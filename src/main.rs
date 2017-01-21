extern crate iron;
extern crate router;
extern crate rustc_serialize;
extern crate hyper;
extern crate crossbeam;
extern crate classifier;

use iron::prelude::*;
use iron::status;
use router::Router;
use rustc_serialize::json;
use std::io::Read;
use std::sync::{Arc, Mutex};
use hyper::Client;
use classifier::NaiveBayes;
use std::thread;
use std::sync::mpsc;
use std::fs::File;
use std::io::Write;

#[derive(RustcEncodable, RustcDecodable)]
struct Greeting {
    msg: String
}

#[derive(RustcEncodable)]
struct Greetings<'a> {
    allMsgs: &'a Vec<String>
}

fn main() {

    let mut router = Router::new();
    let greeting = Arc::new(Mutex::new(Greeting { msg: "Hello, World".to_string() }));
    let greeting_clone = greeting.clone();
    let client = Client::new();

    router.get("/", move |r: &mut Request| hello_world(r, &greeting.lock().unwrap()), "index");
    router.post("/set", move |r: &mut Request| set_greeting(r, &mut greeting_clone.lock().unwrap()), "set");
    router.get("/web", move |r: &mut Request| webber(r, &client), "webber");
    router.get("/learn", move |r: &mut Request| learn(r), "learn");

    fn hello_world(_ : &mut Request, greeting: &Greeting) -> IronResult<Response> {
        let payload = json::encode(&greeting).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }

    fn set_greeting(request: &mut Request, greeting: &mut Greeting) -> IronResult<Response> {
        let mut payload = String::new();
        request.body.read_to_string(&mut payload).unwrap();
        *greeting = json::decode(&payload).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }

    fn webber(_: & mut Request, c: & Client) -> IronResult<Response> {
        //let mut data : Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        for _ in 0..10 {
            let tx = tx.clone();
            //let c_clone = c.clone();
            crossbeam::scope(|scope| {
                scope.spawn(move || {
                    let result = goGetEm(c);
                    tx.send(result).unwrap();
                });
            });
        }
        let mut data : Vec<String> = Vec::new();
        for _ in 0..10 {
            //data.lock().unwrap().push(rx.recv().unwrap());
            data.push(rx.recv().unwrap());
        }
        let greeting = Greetings { allMsgs: &data };
        let payload = json::encode(&greeting).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }

    fn goGetEm(c: &Client) -> String {
        let mut res = c.get("http://stackoverflow.com/").send().unwrap();
        assert_eq!(res.status, hyper::Ok);
        let mut s = String::new();
        res.read_to_string(&mut s).unwrap();
        return s;
    }

    fn learn(_: &mut Request) -> IronResult<Response> {
        let mut nb = NaiveBayes::new();
        let examples = [
            ("beetroot water spinach okra water chestnut ricebean pea catsear courgette summer purslane. water spinach arugula pea tatsoi aubergine spring onion bush tomato kale radicchio turnip chicory salsify pea sprouts fava bean. dandelion zucchini burdock yarrow chickpea dandelion sorrel courgette turnip greens tigernut soybean radish artichoke wattle seed endive groundnut broccoli arugula.", "veggie"),
            ("sirloin meatloaf ham hock sausage meatball tongue prosciutto picanha turkey ball tip pastrami. ribeye chicken sausage, ham hock landjaeger pork belly pancetta ball tip tenderloin leberkas shank shankle rump. cupim short ribs ground round biltong tenderloin ribeye drumstick landjaeger short loin doner chicken shoulder spare ribs fatback boudin. pork chop shank shoulder, t-bone beef ribs drumstick landjaeger meatball.", "meat"),
            ("pea horseradish azuki bean lettuce avocado asparagus okra. kohlrabi radish okra azuki bean corn fava bean mustard tigernut jã­cama green bean celtuce collard greens avocado quandong fennel gumbo black-eyed pea. grape silver beet watercress potato tigernut corn groundnut. chickweed okra pea winter purslane coriander yarrow sweet pepper radish garlic brussels sprout groundnut summer purslane earthnut pea tomato spring onion azuki bean gourd. gumbo kakadu plum komatsuna black-eyed pea green bean zucchini gourd winter purslane silver beet rock melon radish asparagus spinach.", "veggie"),
            ("sirloin porchetta drumstick, pastrami bresaola landjaeger turducken kevin ham capicola corned beef. pork cow capicola, pancetta turkey tri-tip doner ball tip salami. fatback pastrami rump pancetta landjaeger. doner porchetta meatloaf short ribs cow chuck jerky pork chop landjaeger picanha tail.", "meat"),
        ];
        for &(document, label) in examples.iter() {
            nb.add_document(&document.to_string(), &label.to_string());
        }
        nb.train();
        let trained_content = nb.to_json();

        match File::create("foo.txt") {
            Ok(mut f) => {
                f.write_all(trained_content.as_bytes());
            }
            Err(err) => panic!("Unable to open file!")
        };

        let mut s = String::new();
        match File::open("foo.txt") {
            Ok(mut f) => {
                f.read_to_string(&mut s)
            }
            Err(err) => panic!("Unable to open file!")
        };

        let mut nb2 = NaiveBayes::from_json(&s);
        let food_document = "salami pancetta beef ribs".to_string();
        let greeting = Greeting {msg: nb2.classify(&food_document)};
        let payload = json::encode(&greeting).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }

    Iron::new(router).http("localhost:3000").unwrap();
    println!("On 3000");
}
