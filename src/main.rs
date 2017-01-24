extern crate iron;
extern crate router;
extern crate rustc_serialize;
extern crate hyper;
extern crate crossbeam;
extern crate classifier;
extern crate select;

use iron::prelude::*;
use iron::status;
use router::Router;
use rustc_serialize::json;
use std::io::Read;
use std::sync::{Arc, Mutex};
use hyper::Client;
use hyper::Url;
use classifier::NaiveBayes;
use std::thread;
use std::sync::mpsc;
use std::fs::File;
use std::io::Write;
use select::document::Document;
use select::predicate::{Predicate, Attr, Class, Name};

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
    router.get("/learn", move |r: &mut Request| learn(r), "learn");
    router.get("/parseit", move |r: &mut Request| parse_it(r, &client), "parse");


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

    fn parse_it(_: &mut Request, c: & Client) -> IronResult<Response> {
        let mut res = c.get("http://streeteasy.com/for-rent/nyc/price:3500-4500%7Carea:115,116,107,105,157,364,322,304%7Cbeds:2%7Cinterestingatint%3E1469787712").send().unwrap();
        assert_eq!(res.status, hyper::Ok);

        let mut hrefs : Vec<String> = Vec::new();

        let mut s = String::new();
        res.read_to_string(&mut s).unwrap();

        let document = Document::from(&*s);

        for node in document.find(Class("details-title")).iter() {
            let a = node.find(Name("a")).first().unwrap().attr("href").unwrap().to_string();
            hrefs.push(a);
        }

        let results = webber(c, hrefs);

        let greeting = Greetings { allMsgs: &results };
        let payload = json::encode(&greeting).unwrap();
        Ok(Response::with((status::Ok, payload)))

    }

    fn webber(c: & Client, apartments: Vec<String>) -> Vec<String> {
        let mut s = String::new();
        match File::open("foo.txt") {
            Ok(mut f) => {
                f.read_to_string(&mut s)
            }
            Err(err) => panic!("Unable to open file!")
        };

        let mut nb = NaiveBayes::from_json(&s);

        let (tx, rx) = mpsc::channel();
        for apt in apartments.clone() {
            let tx = tx.clone();
            let nb2 = nb.clone();
            crossbeam::scope(|scope| {
                scope.spawn(move || {
                    let result = goGetEm(c, apt, nb2);
                    tx.send(result).unwrap();
                });
            });
        }
        let mut data : Vec<String> = Vec::new();
        for apt in apartments.clone() {
            let verified = rx.recv().unwrap();
            if(verified == "true") {
                let url = Url::parse("http://streeteasy.com").unwrap();
                let url = url.join(&*apt).unwrap().as_str().to_string();
                data.push(url);
            }
        }

        return data;
    }

    fn goGetEm(c: &Client, extension: String, nb: NaiveBayes) -> String {
        let url = Url::parse("http://streeteasy.com").unwrap();
        let url = url.join(&*extension).unwrap();

        let mut res = c.get(url.as_str()).send().unwrap();
        assert_eq!(res.status, hyper::Ok);
        let mut s = String::new();
        res.read_to_string(&mut s).unwrap();

        let document = Document::from(&*s);

        let block = document.find(Class("listings_sections")).first().unwrap();
        let body = block.find(Name("blockquote")).first().unwrap().text();

        return nb.classify(&s);
    }


    fn learn(_: &mut Request) -> IronResult<Response> {
        let mut nb = NaiveBayes::new();
        let examples = [
            ("Gross $3,791 / Net $3,500 (this reflects 1 month free given on the last month)Renovated 2BR + Den/1BTH in Clinton Hill in an elevator building with laundry on site! This stunning home is equipped with: Stainless steel appliances Large separate kitchen Ample living space Separate dining room C train at Clinton-Washington : .22 miles G train at Clinton-Washington: .2 milesNote: These are not pictures of the actual home; pictures shown are of a similar home with the same finishes/appliances", "true"),
            ("Great two bedroom apartment situated in the heart of Chelsea. NEW APPLIANCES included in the lease. Lots of light enter through the large windows which overlook the landscaped courtyard. There is a live-in super, laundry on premise, and the building is wired for FIOS. Your new home awaits - Sorry no dogs. Email for more information.", "true"),
            ("Sprawling Apartment in BrownstoneBeautiful light and space! Top floor, loft style Dormer Windows. Renovated kitchen with stone counters and stainless steel appliances. Recently renovated bathroom with stand up Shower. Fireplace. Washer/dryer Two Large bedrooms. Streaming sunlight! Beautifully kept brownstone in prime location! No pets please.", "false"),
            ("AVAILABLE starting February 1st. Newly renovated 2-bedroom in West Village. Bedrooms are on separate sides of the apartment. Kitchen has all new stainless steel appliances- dishwasher, microwave, stove/oven, and fridge. There is recessed lighting throughout, as well as historic charm such as an exposed brick wall and wood floors. The West Village location (right off the 1 train) can't be beat, and because this unit faces the back of the building, it is surprisingly quiet. All the best restaurants, fun bars, and great shopping are at your fingertips, and you are a 5 minute walk from the Hudson River. App screening charge of $400. No Broker Fee!! ", "false"),
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
