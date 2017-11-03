extern crate rand;
use rand::Rng;

fn rand_string() -> String {
    rand::thread_rng().gen_ascii_chars().take(16).collect()
}

struct Part {
    name: String,
    content_type: Option<String>,
    data: Vec<u8>,
}

impl Part {
    pub fn new(name: String, content_type: Option<String>, data: Vec<u8>) -> Part {
        Part {
            name : name,
            content_type: content_type,
            data : data,
        }
    }
}

pub struct MultiPart {
    parts: Vec<Part>,
    id : String,
}

impl MultiPart {
    pub fn new() -> MultiPart {
        MultiPart {
            parts: Vec::new(),
            id: rand_string(),
        }
    }

    pub fn part<'a>(&'a mut self, name: &str, content_type: Option<String>, data: Vec<u8>) -> &'a mut MultiPart {
        let part = Part::new(name.to_owned(), content_type, data);
        self.parts.push(part);
        self
    }

    pub fn str_part<'a>(&'a mut self, name: &str, content_type: Option<String>, data: &str) -> &'a mut MultiPart {
        let part = Part::new(name.to_owned(), content_type, data.to_owned().into_bytes());
        self.parts.push(part);
        self
    }

    pub fn to_content_type<'a>(&'a mut self) -> String {
        format!("multipart/form-data; boundary=------------------------{}", self.id)
    }

    pub fn to_raw<'a>(&'a mut self) -> Vec<u8> {
        let mut raw = Vec::new();
        let header = format!("--------------------------{}", self.id);
        let bytes = header.as_bytes();
        for part in self.parts.iter() {
            raw.extend(bytes);
            raw.extend(format!("\r\nContent-Disposition: form-data; name=\"{}\"\r\n", part.name).as_bytes());
            match part.content_type {
                Some(ref t) => raw.extend(format!("Content-Type: {}\r\n", t).as_bytes()),
                None  => raw.extend("\r\n".as_bytes()),
            }
            raw.extend(part.data.clone());
            raw.extend("\r\n".as_bytes());
        }
        raw.extend(bytes);
        raw.extend("--\r\n".as_bytes());
        raw
    }
}
