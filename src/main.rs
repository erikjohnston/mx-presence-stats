#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]

#[macro_use] extern crate clap;
extern crate hyper;
extern crate rustc_serialize;
extern crate serde;
extern crate serde_json;
extern crate toml;
extern crate chrono;

use std::fs::File;
use std::io::{Read, Write};

use chrono::Timelike;


#[derive(Serialize, Deserialize, Debug)]
struct PresenceEvent {
    sender: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct PresenceBlock {
    events: Vec<PresenceEvent>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SyncResponse {
    next_batch: String,
    presence: PresenceBlock,
}

#[derive(Deserialize, Debug)]
struct Config {
    server_url: String,
    access_token: String,
}


fn main() {
    let matches = clap::App::new("matrix-bot")
        .version(crate_version!())
        .author("Erik Johnston <mxfedtest@jki.re>")
        .setting(clap::AppSettings::ArgRequiredElseHelp)
        .arg(clap::Arg::with_name("CONFIG")
            .required(true)
        )
        .arg(clap::Arg::with_name("output")
            .required(false)
        )
        .get_matches();

    let config : Config = {
        let mut f = File::open(matches.value_of("CONFIG").unwrap()).unwrap();
        let mut config_str = String::new();
        f.read_to_string(&mut config_str).unwrap();

        let mut parser = toml::Parser::new(&config_str);
        let value = match parser.parse() {
            Some(v) => v,
            None => {
                panic!("{}", parser.errors[0]);
            }
        };
        let mut d = toml::Decoder::new(toml::Value::Table(value));
        serde::Deserialize::deserialize(&mut d).unwrap()
    };

    println!("Config: {:#?}", config);

    let output_file = matches.value_of("output").map(|fname| File::create(fname).unwrap());

    let client = hyper::Client::new();

    let mut next_batch = {
        let init_sync_url = format!(
            "{}/_matrix/client/r0/sync?access_token={}&timeout=30000",
            &config.server_url,
            &config.access_token,
        );
        let res = client.get(&init_sync_url).send().unwrap();
        let sync_response: SyncResponse = serde_json::from_reader(res).unwrap();
        sync_response.next_batch
    };

    println!("Started!");

    let mut presence_freq = PresenceFreq::with_file(output_file);

    loop {
        let sync_url = format!(
            "{}/_matrix/client/r0/sync?access_token={}&since={}&timeout=30000",
            &config.server_url,
            &config.access_token,
            &next_batch,
        );

        let res = client.get(&sync_url).send().unwrap();

        let sync_response: SyncResponse = serde_json::from_reader(res).unwrap();

        presence_freq.handle_response(&sync_response);

        next_batch = sync_response.next_batch;
    }
}


struct PresenceFreq {
    last_presence: Option<chrono::DateTime<chrono::UTC>>,
    output_file: Option<File>,
}

impl PresenceFreq {
    fn with_file(output_file: Option<File>) -> PresenceFreq {
        PresenceFreq {
            last_presence: None,
            output_file: output_file,
        }
    }

    fn handle_response(&mut self, sync_response: &SyncResponse) {
        if !sync_response.presence.events.is_empty() {
            let now = chrono::UTC::now();
            let num_events = sync_response.presence.events.len();

            if let Some(last) = self.last_presence {
                let ms = (now - last).num_milliseconds();
                println!("Got presence {} event(s): {:>6}ms since last presence", num_events, ms);

                let timestamp = (now.timestamp() * 1000) as u64 + (now.nanosecond() / 1000000) as u64;

                if let Some(ref mut file) = self.output_file {
                    writeln!(file, "{} {} {}", timestamp, ms, num_events).unwrap();
                }
            } else {
                println!("Got presence {} event(s)", num_events);
            }
            self.last_presence = Some(now);
        }
    }
}
