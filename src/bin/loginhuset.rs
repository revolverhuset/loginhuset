extern crate futures;
extern crate getopts;
#[macro_use]
extern crate hyper;
extern crate hyper_staticfile;
extern crate hyper_tls;
#[macro_use]
extern crate log;
extern crate multipart;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_urlencoded;
extern crate simplelog;
extern crate tokio_core;
extern crate diesel;
extern crate loginhuset;
#[macro_use]
extern crate lazy_static;
extern crate rand;
extern crate url;
extern crate toml;

use loginhuset::*;
use loginhuset::models::*;
use diesel::prelude::*;
use futures::Stream;
use futures::future::Future;
use getopts::Options;
use hyper::server::{Http, Request, Response, Service};
use hyper::Client;
use hyper_tls::HttpsConnector;
use hyper::{Method, StatusCode};
use hyper::header::{ContentLength, Authorization, Basic, Cookie, SetCookie, Location};
use hyper_staticfile::Static;
use simplelog::{TermLogger};
use std::path::Path;
use tokio_core::reactor::{Core, Handle, Timeout};
use tokio_core::net::TcpListener;
use std::rc::Rc;
use std::time::Duration;
use std::sync::Mutex;
use std::fs::File;

lazy_static! {
    static ref TOKENS: Mutex<std::collections::HashMap<String, User>> = {
        let m = std::collections::HashMap::new();
        Mutex::new(m)
    };
}

#[derive(Deserialize)]
struct Config {
    database: String,
    port: Option<u16>,
    www_root: String,
    mailgun: Mailgun,
}

#[derive(Deserialize)]
struct Mailgun {
    api_key: String,
    from: String,
    subject: String,
    html_template: String,
    text_template: String,
}

header! { (XLimitExcept, "X-Limit-Except") => (hyper::Method)* }
header! { (XRequestMethod, "X-Request-Method") => [hyper::Method] }

struct SimpleServer {
    static_: Static,
    client: Rc<Client<HttpsConnector<hyper::client::HttpConnector>>>,
    handle: Handle,
    mailgun: Rc<Mailgun>,
    db_connection: Rc<SqliteConnection>,
}

impl SimpleServer {
    fn new(
        handle: &Handle,
        base_path: &str,
        mailgun: Rc<Mailgun>,
        db: Rc<SqliteConnection>
    ) -> SimpleServer {
        SimpleServer {
            static_: Static::new(handle, Path::new(base_path)),
            client: Rc::new(
                ::hyper::Client::configure()
                    .connector(HttpsConnector::new(4, handle).unwrap())
                    .build(handle),
            ),
            mailgun: mailgun,
            handle: handle.clone(),
            db_connection: db,
        }
    }
}

fn rand_string() -> String {
    use rand::{OsRng, Rng};
    let mut gen = OsRng::new().ok().expect("Failed to get OS random generator");

    gen.gen_ascii_chars().take(32).collect()
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

fn mailgun_request(email: &str, config: Rc<Mailgun>, url: &str) -> hyper::Request {
    let (content_type, data) = multipart(&*config, email, url);
    let uri = "https://api.mailgun.net/v3/mg.revolverhuset.no/messages"
        .parse()
        .unwrap();
    let mut req: hyper::Request = Request::new(Method::Post, uri);
    req.headers_mut().set(ContentLength(data.len() as u64));
    req.headers_mut().set(Authorization(Basic {
        username: "api".to_owned(),
        password: Some(config.api_key.clone()),
    }));
    req.headers_mut().set_raw("content-type", content_type);
    req.set_body(data);
    req
}

fn get_user(user_email: &str, db_conn: &SqliteConnection) -> Option<User> {
    use ::loginhuset::schema::users::dsl::*;
    users.filter(email.eq(user_email))
        .first::<User>(&*db_conn)
        .optional()
        .expect("Failed to find users table")
}

fn create_timer_token(token: String, handle: &Handle) {
    let timer = Timeout::new(Duration::from_secs(15 * 60), handle)
        .unwrap()
        .and_then(move |_| {
            TOKENS.lock().unwrap().remove(&token);
            Ok(())
        })
        .map_err(|e| {
            error!("[timer] {}", e);
            ()
        });
    handle.spawn(timer);
}

fn send_authentication_email(
    client: &Client<HttpsConnector<hyper::client::HttpConnector>>,
    req: hyper::Request,
    handle: &Handle
) {
    let client_future = client
        .request(req)
        .and_then(|res| {
            info!("[mailgun] status: {}", res.status());
            res.body().concat2()
        })
        .and_then(|body| {
            info!(
                "[mailgun] response {}",
                String::from_utf8(body.to_vec()).unwrap_or("[Invalid utf-8]".to_owned())
                    );
            Ok(())
        })
        .map_err(|e| {
            error!("[mailgun] {}", e);
            ()
        });
    handle.spawn(client_future);
}

fn handle_authenticate(
    req: Request,
    handle: Handle,
    mailgun: Rc<Mailgun>,
    db_conn: Rc<SqliteConnection>,
    client: Rc<Client<HttpsConnector<hyper::client::HttpConnector>>>,
) -> Box<Future<Item = hyper::Response, Error = hyper::Error>> {

    #[derive(Deserialize)]
    struct LoginRequest {
        email: String,
    }

    let origin = {
        let args = get_query_map(req.query()).unwrap_or(std::collections::HashMap::new());
        args.get("origin").as_ref().map(|x| &x[..]).unwrap_or("/").to_owned()
    };

    let response_future = req.body()
        .concat2()
        .map_err(Into::into)
        .and_then(move |body| {
            serde_urlencoded::from_bytes(&body)
                .map_err(Into::<Box<::std::error::Error>>::into)
        }).and_then(move |lr: LoginRequest| {
            let mut response = Response::new();
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

                create_timer_token(token, &handle);
                send_authentication_email(&*client, req, &handle);
            }
            response.set_status(StatusCode::Ok);
            response.set_body("Epost (kanskje) sendt.");
            Ok(response)
        })
        .or_else(|err| -> Result<_, hyper::Error> {
            warn!("{}", err);
            let mut response = Response::new();
            response.set_status(StatusCode::InternalServerError);
            Ok(response)
        });

    Box::new(response_future)
}

fn get_query_map(query_string: Option<&str>) -> Option<std::collections::HashMap<String, String>> {
    use url::form_urlencoded::parse;
    query_string.map(|qs| parse(qs.as_bytes()).into_owned().collect())
}

fn handle_logout(
    session: Option<(Session, User)>,
    db_conn: &SqliteConnection,
) -> Box<Future<Item = hyper::Response, Error = hyper::Error>> {
    use loginhuset::schema::sessions::dsl::*;

    if let Some((s, _)) = session {
        diesel::delete(sessions.filter(token.eq(s.token)))
            .execute(db_conn)
            .expect("DB error");
    }
    let mut response = Response::new();
    response.set_status(StatusCode::Ok);
    Box::new(futures::future::ok(response))
}

fn handle_validate(
    query_string: Option<&str>,
    db_conn: &SqliteConnection,
) -> Box<Future<Item = hyper::Response, Error = hyper::Error>> {
    let mut response = Response::new();
    let args = get_query_map(query_string).unwrap_or(std::collections::HashMap::new());
    let ot = args.get("token")
        .and_then(|t| TOKENS.lock().unwrap().remove(t))
        .map(|t| (t, args.get("origin")));
    match ot {
        Some((user, origin)) => {
            info!("Validated {}, setting cookie.", user.email);
            let token = rand_string();
            create_session(db_conn, &user, &token);

            response.headers_mut().set(SetCookie(vec![
                format!(
                    "revolverhuset={}; Path=/; Max-Age=31536000",
                    token
                ),
            ]));
            response.headers_mut().set(Location::new(
                origin.as_ref().map(|x| &x[..]).unwrap_or("/").to_owned(),
            ));
            response.set_status(StatusCode::TemporaryRedirect);
            response.set_body("Logged in!");
            return Box::new(futures::future::ok(response));
        }
        None => {
            response.set_status(StatusCode::BadRequest);
        }
    }
    Box::new(futures::future::ok(response))
}

fn check_cookie(
    cookie_header: &Option<&Cookie>,
    db_conn: &SqliteConnection,
) -> Option<(Session, User)> {
    use loginhuset::schema::sessions::dsl::*;
    use schema::{users, sessions};

    cookie_header
        .and_then(|c| c.get("revolverhuset"))
        .and_then(|value| {
            sessions::table
                .inner_join(users::table)
                .filter(token.eq(value))
                .first::<(Session, User)>(db_conn)
                .optional()
                .expect("Failed to load data from DB.")
        })
}

impl Service for SimpleServer {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        info!("Request [{}] {} {}", req.method(), req.path(), req.query().unwrap_or("<>"));
        let cookie = check_cookie(&req.headers().get::<Cookie>(), &*self.db_connection);

        let is_whitelisted = match (
            req.headers().get::<XLimitExcept>(),
            req.headers().get::<XRequestMethod>()
        ) {
            (Some(limit_except), Some(request_method)) =>
                limit_except.contains(&request_method),
            _ => false
        };

        match (req.method(), req.path()) {
            (&Method::Get, "/_authentication/check") => {
                let mut response = Response::new();
                match (cookie, is_whitelisted) {
                    (Some((_, user)), _) => {
                        response.headers_mut().set_raw("x-identity", user.name);
                        response.headers_mut().set_raw("x-user", user.email);
                        response.set_status(StatusCode::Ok);
                    }
                    (_, true) => {
                        response.set_status(StatusCode::Ok);
                    }
                    (None, _) => {
                        response.set_status(StatusCode::Unauthorized);
                    }
                }
                Box::new(futures::future::ok(response))
            }
            (&Method::Post, "/_authentication/authenticate") => {
                handle_authenticate(
                    req,
                    self.handle.clone(),
                    Rc::clone(&self.mailgun),
                    Rc::clone(&self.db_connection),
                    Rc::clone(&self.client),
                )
            }
            (&Method::Get, "/_authentication/logout") => handle_logout(cookie, &*self.db_connection),
            (&Method::Get, "/_authentication/validate") => handle_validate(req.query(), &*self.db_connection),
            (&Method::Get, _) => self.static_.call(req),
            (_, _) => {
                let mut response = Response::new();
                response.set_status(StatusCode::MethodNotAllowed);
                Box::new(futures::future::ok(response))
            }
        }
    }
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

fn main() {
    let matches = args();

    TermLogger::init(
        matches
            .opt_str("l")
            .unwrap_or("info".to_owned())
            .parse()
            .unwrap(),
        simplelog::Config::default(),
    ).unwrap();

    let config = load_config(&matches.opt_str("c").unwrap());
    if !Path::new(&config.mailgun.html_template).is_file() {
        panic!("Html template does not exist.");
    }

    if !Path::new(&config.mailgun.text_template).is_file() {
        panic!("Text template does not exist.");
    }

    if !Path::new(&config.www_root).is_dir() {
        panic!("Static content path does not exist.");
    }

    let addr = format!("127.0.0.1:{}", config.port.unwrap_or(3000)).parse().unwrap();
    let db_conn = Rc::new(establish_connection(&config.database));

    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let listener = TcpListener::bind(&addr, &handle).unwrap();
    let http = Http::new();
    let www_root = &config.www_root;
    let rc_mg = Rc::new(config.mailgun);

    let server = listener.incoming().for_each(|(sock, addr)| {
        let s = SimpleServer::new(
            &handle,
            &www_root,
            Rc::clone(&rc_mg),
            Rc::clone(&db_conn));
        http.bind_connection(&handle, sock, addr, s);
        Ok(())
    });

    info!("Listening on {}", addr);
    core.run(server).unwrap()
}
