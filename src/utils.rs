use std::fs::File;

pub fn render(template: &str, url: &str) -> String {
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

pub fn rand_string() -> String {
    use rand::distributions::Alphanumeric;
    use rand::Rng;

    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .collect::<String>()
}
