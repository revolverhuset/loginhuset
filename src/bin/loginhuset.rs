use serde::Deserialize;
use getopts::Options;
use simplelog::{TermLogger};
use std::fs::File;
use std::path::Path;
use std::rc::Rc;
use diesel::sqlite::SqliteConnection;
use diesel::prelude::*;
use cookie::Cookie;
use log::{info, error};
use base64::{encode};
use lazy_static::lazy_static;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Client, Body, Error, Request, Response, Server, Method, StatusCode};
use hyper_staticfile::Static;
use hyper_tls::HttpsConnector;

use loginhuset::*;
use loginhuset::models::*;

lazy_static! {
    static ref TOKENS: std::sync::Mutex<std::collections::HashMap<String, User>> = {
        let m = std::collections::HashMap::new();
        std::sync::Mutex::new(m)
    };
}

#[derive(Deserialize)]
struct Mailgun {
    api_key: String,
    from: String,
    subject: String,
    html_template: String,
    text_template: String,
}


#[derive(Deserialize)]
struct Config {
    cookie_name: String,
    database: String,
    port: Option<u16>,
    www_root: String,
    mailgun: Mailgun,
}

#[derive(Clone, Copy, Debug)]
struct LocalExec;
impl<F> hyper::rt::Executor<F> for LocalExec
where
    F: std::future::Future + 'static, // not requiring `Send`
{
    fn execute(&self, fut: F) {
        tokio::task::spawn_local(fut);
    }
}

fn load_config(path: &str) -> Config {
    let data = {
        use std::io::Read;
        let mut s = String::new();
        let f = File::open(path);
        if f.is_err() {
            panic!("Failed to read config '{}'", path);
        }
        let mut f = f.unwrap();
        f.read_to_string(&mut s).unwrap();
        s
    };
    toml::from_str(&data[..]).unwrap()
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} FILE [options]", program);
    print!("{}", opts.usage(&brief));
}

fn args() -> getopts::Matches {
    use std::env::args;

    let args: Vec<String> = args().collect();
    let program = args[0].clone();
    let mut opts = Options::new();
    opts.optflag("h", "help", "Print usage");
    opts.optopt("l", "log-level", "Log level", "LEVEL");
    opts.reqopt("c", "config", "Configuration file", "TOML");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            print_usage(&program, opts);
            panic!(f.to_string());
        }
    };

    if matches.opt_present("h") {
        print_usage(&program, opts);
        std::process::exit(1)
    }

    matches
}

fn render(template: &str, url: &str) -> String {
    let data = {
        use std::io::Read;
        let mut s = String::new();
        let f = File::open(template);
        if f.is_err() {
            panic!("Failed to load template '{}'", template);
        }
        let mut f = f.unwrap();
        f.read_to_string(&mut s).unwrap();
        s
    };
    data.replace("{{url}}", url)
}

fn multipart(config: &Mailgun, email: &str, url: &str) -> (String, Vec<u8>) {
    let mut mp = multipart::MultiPart::new();
    mp.str_part("from", None, &config.from);
    mp.str_part("to", None, email);
    mp.str_part("subject", None, &config.subject);
    mp.str_part("text", None, &render(&config.text_template, url));
    mp.str_part("html", None, &render(&config.html_template, url));
    (mp.to_content_type(), mp.to_raw())
}

fn check_cookie(
    cookie_header: &Option<&str>,
    cookie_name: &str,
    db_conn: &SqliteConnection,
) -> Option<(Session, User)> {
    use loginhuset::schema::sessions::dsl::*;
    use schema::{users, sessions};

    cookie_header
        .and_then(|c| Cookie::parse(c).ok())
        .filter(|c| c.name().eq(cookie_name))
        .and_then(|c| {
            sessions::table
                .inner_join(users::table)
                .filter(token.eq(c.value()))
                .first::<(Session, User)>(db_conn)
                .optional()
                .expect("Failed to load data from DB.")
        })
}

fn delete_session(
    session: Option<(Session, User)>,
    db_conn: &SqliteConnection,
) {
    use loginhuset::schema::sessions::dsl::*;

    if let Some((s, _)) = session {
        diesel::delete(sessions.filter(token.eq(s.token)))
            .execute(db_conn)
            .expect("DB error");
    }
}

fn get_user(user_email: &str, db_conn: &SqliteConnection) -> Option<User> {
    use ::loginhuset::schema::users::dsl::*;
    users.filter(email.like(user_email))
        .first::<User>(&*db_conn)
        .optional()
        .expect("Failed to find users table")
}

fn mailgun_request(email: &str, config: &Mailgun, url: &str) -> hyper::Request<Body> {
    let (content_type, data) = multipart(&*config, email, url);

    Request::builder()
        .method(Method::POST)
        .uri("https://api.mailgun.net/v3/mg.revolverhuset.no/messages")
        .header("authorization", format!("Basic {}", encode(format!("api:{}", config.api_key))))
        .header("content-type", content_type)
        .header("content-length", data.len() as u64)
        .body(Body::from(data)).unwrap()
}

fn get_query_map(query_string: Option<&str>) -> Option<std::collections::HashMap<String, String>> {
    use url::form_urlencoded::parse;
    query_string.map(|qs| parse(qs.as_bytes()).into_owned().collect())
}

fn rand_string() -> String {
    use rand::Rng; 
    use rand::distributions::Alphanumeric;

    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .collect::<String>()
}

fn authenticate(body: Vec<u8>, origin: String, db_conn: &SqliteConnection, mailgun: &Mailgun) -> Result<(), anyhow::Error> {
    #[derive(Deserialize)]
    struct LoginRequest {
        email: String,
    }

    // A request that failed to decode is a bad request
    let lr = serde_urlencoded::from_bytes::<LoginRequest>(&body)?;

    // However, not having a user is an OK result, to avoid leaking info.
    if let Some(user) = get_user(&lr.email, &*db_conn) {
        info!("User: {} {}", user.email, user.name);
        let token = rand_string();
        let url = format!(
            "https://revolverhuset.no/_authentication/validate?token={}&origin={}",
            token,
            origin
        );
        let req = mailgun_request(&user.email, mailgun, &url);

        TOKENS.lock().unwrap().insert(token.clone(), user);

        tokio::spawn(async move {
            tokio::time::delay_for(std::time::Duration::from_secs(60 * 15)).await;
            TOKENS.lock().unwrap().remove(&token);
        });

        tokio::spawn(async move {
            let https = HttpsConnector::new();
            let client = Client::builder().build::<_, hyper::Body>(https);
            let res = client.request(req).await;
            match res {
                Ok(_) => info!("Successfully sent email to {}", lr.email),
                Err(e) => error!("Failed to send email to {}, got {}", lr.email, e)
            }
        });
    }
    Ok(())
}

async fn route_request(req: Request<Body>, fsstatic: Static, db_connection: Rc<SqliteConnection>, config: Rc<Config>) -> Result<Response<Body>, anyhow::Error> {
    info!("Request [{}] {} {}", req.method(), req.uri().path(), req.uri().query().unwrap_or("<>"));
    let cookie_name = &config.cookie_name;
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/_authentication/check") => {
            let cookies = check_cookie(&req.headers().get(hyper::header::COOKIE).map(|h| h.to_str().unwrap()), cookie_name, &*db_connection);
            Ok(cookies
               .map_or_else(
                   || Response::builder()
                       .status(401)
                       .body(Body::empty())
                       .unwrap(),
                   |(_, user)| Response::builder()
                       .status(200)
                       .header("x-identity", user.name)
                       .header("x-user", user.email)
                       .body(Body::empty())
                       .unwrap()))
        }
        (&Method::GET, "/_authentication/logout") => {
            let cookies = check_cookie(&req.headers().get(hyper::header::COOKIE).map(|h| h.to_str().unwrap()), cookie_name, &*db_connection);
            delete_session(cookies, &*db_connection);
            Ok(Response::builder()
               .status(200)
               .body(Body::empty())
               .unwrap())
        }
        (&Method::GET, "/_authentication/validate") => {
            let args = get_query_map(req.uri().query()).unwrap_or(std::collections::HashMap::new());
            let validation = args.get("token").and_then(|token| TOKENS.lock().unwrap().remove(token)).map(|user| {
                info!("Validated {}, setting cookie.", user.email);
                let cookie = rand_string();
                create_session(&*db_connection, &user, &cookie);
                cookie
            });

            match validation {
                Some(cookie) => Ok(Response::builder()
                    .status(307)
                    .header("Set-Cookie", format!("{}={}; Path=/; Max-Age=31536000", cookie_name, cookie))
                    .header("location", args.get("origin").as_ref().map(|x| &x[..]).unwrap_or("/").to_owned())
                    .body(Body::empty())
                    .unwrap()),
                None => Ok(Response::builder()
                    .status(400)
                    .body(Body::empty())
                    .unwrap())
            }
        }
        (&Method::POST, "/_authentication/authenticate") => {
            let origin = {
                let args = get_query_map(req.uri().query()).unwrap_or(std::collections::HashMap::new());
                args.get("origin").as_ref().map(|x| &x[..]).unwrap_or("/").to_owned()
            };
            let body_data = hyper::body::to_bytes(req.into_body()).await?;

            let result = authenticate(body_data.to_vec(), origin, &*db_connection, &(&*config).mailgun);
            match result {
                Ok(_) => Ok(Response::builder().status(200).body(Body::empty()).unwrap()),
                Err(_) => Ok(Response::builder().status(400).body(Body::empty()).unwrap())
            }
        }
        (&Method::GET, _) => {
            fsstatic.clone().serve(req).await.map_err(Into::into)
        }
        _ => {
            let mut res = Response::new(Body::empty());
            *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
            Ok(res)
        },
    }
}

async fn run(config: Rc<Config>) {
    let addr = format!("127.0.0.1:{}", config.port.unwrap_or(3000)).parse().unwrap();

    let db_conn = Rc::new(establish_connection(&*config.database));
    let fsstatic = hyper_staticfile::Static::new(&config.www_root);

    let make_service = make_service_fn(move |_| {
        let config = config.clone();
        let db_conn = db_conn.clone();
        let fsstatic = fsstatic.clone();
        async move { Ok::<_, Error>(service_fn(move |req| route_request(req, fsstatic.clone(), db_conn.clone(), config.clone())))}
    });

    let server = Server::bind(&addr).executor(LocalExec).serve(make_service);

    info!("Listening on http://{}", addr);

    if let Err(e) = server.await {
        error!("server error: {}", e);
    }
}

fn main() {
    let matches = args();

    TermLogger::init(
        matches
            .opt_str("l")
            .unwrap_or("info".to_owned())
            .parse()
            .unwrap(),
        simplelog::Config::default(),
        simplelog::TerminalMode::Stderr
    ).unwrap();

    let config = Rc::new(load_config(&matches.opt_str("c").unwrap()));

    if !Path::new(&config.mailgun.html_template).is_file() {
        panic!("Html template does not exist.");
    }

    if !Path::new(&config.mailgun.text_template).is_file() {
        panic!("Text template does not exist.");
    }

    if !Path::new(&config.www_root).is_dir() {
        panic!("Static content path does not exist.");
    }

    let mut rt = tokio::runtime::Builder::new()
        .enable_all()
        .basic_scheduler()
        .build()
        .expect("build runtime");

    let local = tokio::task::LocalSet::new();
    local.block_on(&mut rt, run(config));
}
