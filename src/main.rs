extern crate iron;
extern crate router;
extern crate rustc_serialize;
extern crate hyper;
extern crate crossbeam;
extern crate classifier;
extern crate select;
extern crate time;

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
use std::net::*;
use select::document::Document;
use select::predicate::{Predicate, Attr, Class, Name};
use std::fs::OpenOptions;


#[derive(RustcEncodable, RustcDecodable)]
struct Greeting {
    msg: String
}

#[derive(RustcEncodable, RustcDecodable)]
struct ApartmentUrl {
    url: String,
    label: String
}

#[derive(RustcEncodable, RustcDecodable)]
struct CachedUrl {
    url: String,
    time: i64
}

impl PartialEq for CachedUrl {
    fn eq(&self, other: &CachedUrl) -> bool {
        self.url == other.url
    }
}

impl Eq for CachedUrl {}

unsafe impl Send for ApartmentUrl {}
unsafe impl Sync for ApartmentUrl {}

#[derive(RustcEncodable)]
struct Greetings<'a> {
    allMsgs: &'a Vec<String>
}

#[derive(RustcEncodable)]
pub struct JSON_CONTAINER {
    all_apartments: Vec<u8>,
}

fn main() {
    let mut router = Router::new();
    let greeting = Arc::new(Mutex::new(Greeting { msg: "Hello, World".to_string() }));
    let greeting_clone = greeting.clone();

    let mut model = String::new();
    let mut nb = Arc::new(Mutex::new(NaiveBayes::new()));
    match File::open("foo.txt") {
        Ok(mut f) => {
            f.read_to_string(&mut model);
            nb = Arc::new(Mutex::new(NaiveBayes::from_json(&model)));
        }
        Err(err) => {
            nb = Arc::new(Mutex::new(NaiveBayes::new()));
        }
    };

    let (tx, rx) = mpsc::channel();
    let atx = Arc::new(Mutex::new(tx));
    let nb2 = nb.clone();

    thread::spawn(move || {
        while true {
            let training_client = Client::new();
            let update : ApartmentUrl = rx.recv().unwrap();
            let mut nb2 = nb2.lock().unwrap();

            let mut res = training_client.get(&update.url.to_string()).send().unwrap();
            assert_eq!(res.status, hyper::Ok);
            let mut s = String::new();
            res.read_to_string(&mut s).unwrap();

            let document = Document::from(&*s);

            let block = document.find(Class("listings_sections")).first().unwrap();
            let body = block.find(Name("blockquote")).first().unwrap().text();

            nb2.add_document(&body, &update.label.to_string());
            nb2.train();

            let trained_content = nb2.to_json();

            match File::create("foo.txt") {
                Ok(mut f) => {
                    f.write_all(trained_content.as_bytes());
                }
                Err(err) => panic!("Unable to open file!")
            };
        }
    });

    let nb3 = nb.clone();

    router.get("/", move |r: &mut Request| hello_world(r, &greeting.lock().unwrap()), "index");
    router.post("/set", move |r: &mut Request| set_greeting(r, &mut greeting_clone.lock().unwrap()), "set");
    router.get("/learn", move |r: &mut Request| learn(r), "learn");
    router.post("/train", move |r: &mut Request| train(r, &atx), "train");
    router.get("/parseit", move |r: &mut Request| parse_it(r, &nb3), "parse");


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

    fn parse_it(_: &mut Request, nb : & Arc<Mutex<NaiveBayes>>) -> IronResult<Response> {
        let c = Client::new();
        let current_time = time::get_time().sec;
        let one_day = 86164;
        let yesterday = (current_time - one_day).to_string();

        // homepage for every given day
        let mut url = "http://streeteasy.com/for-rent/nyc/price:3500-4500%7Carea:115,116,107,105,157,364,322,304%7Cbeds:2%7Cinterestingatint%3E".to_string();
        url.push_str(&*yesterday);


        let mut res = c.get(&*url).send().unwrap();
        assert_eq!(res.status, hyper::Ok);


        let mut s = String::new();
        res.read_to_string(&mut s).unwrap();

        let document = Document::from(&*s);

        let mut paginations : Vec<String> = Vec::new();
        for node in document.find(Class("page")).iter() {
            if node.inner_html().len() > 10 {
                paginations.push(node.find(Name("a")).first().unwrap().inner_html());
            }
        }

        let ref last_index = paginations[paginations.len() - 1];
        let number_of_pages: i32 = last_index.parse().unwrap();

        let hrefs = hit_all_pages(&url, number_of_pages);

        let nb = nb.lock().unwrap();

        let results = webber(hrefs, &nb);

        let greeting = Greetings { allMsgs: &results };
        let payload = json::encode(&greeting).unwrap();
        Ok(Response::with((status::Ok, payload)))

    }

    fn hit_all_pages(url: &str, max: i32) -> Vec<String> {
        let mut hrefs : Vec<CachedUrl> = Vec::new();

        let (tx, rx) = mpsc::channel();

        println!("{}", url);

        for (index, value) in (0..max).enumerate() {
            let tx = tx.clone();

            crossbeam::scope(|scope| {
                scope.spawn(move || {
                    let c = Client::new();
                    let current_page = (index + 1).to_string();
                    let mut pagination = "?page=".to_string();
                    pagination.push_str(&current_page);
                    let mut new_url = url.to_string();
                    new_url.push_str(&pagination);
                    let mut res = c.get(&*new_url).send().unwrap();
                    let mut s = String::new();
                    res.read_to_string(&mut s).unwrap();
                    let document = Document::from(&*s);
                    let mut listings = get_listings_on_page(document);
                    tx.send(listings).unwrap();
                });
            });
        }

        for (index, value) in (0..max).enumerate() {
            let resp = rx.recv().unwrap();

            for link in resp {
                if link.contains("?featured=1"){
                    let v: Vec<&str> = link.split("?").collect();

                    if hrefs.iter().find(|ref mut x| x.url == v[0]) == None {
                        let now_time = time::get_time().sec;
                        let link_to_add = link.to_string();
                        let new_href =  CachedUrl {url: link_to_add, time: now_time};
                        &hrefs.push(new_href);
                    }
                }
                else {
                    let curr_time = time::get_time().sec;
                    let new_addition = CachedUrl {url: link.to_string() , time: curr_time};
                    &hrefs.push(new_addition);
                }
            }
        }

        let mut s = String::new();

        let mut f = OpenOptions::new()
                        .read(true)
                        .append(false)
                        .write(true)
                        .open("bar.json").unwrap();

        f.read_to_string(&mut s);
        let mut the_apts : Vec<CachedUrl> = json::decode(&s).unwrap();

        for stuff in hrefs {
            if the_apts.iter().find(|ref mut x| x.url == stuff.url) == None {
            let new_time = time::get_time().sec;
            the_apts.push(stuff);
        }
    }

    write_in_array( &the_apts);

    let mut st = String::new();
    let mut ft = OpenOptions::new()
                        .read(true)
                        .append(false)
                        .write(true)
                        .open("bar.json").unwrap();

    ft.read_to_string(&mut st);

    let mut all_links : Vec<CachedUrl> = json::decode(&st).unwrap();
    let mut list_of_links : Vec<String> = Vec::new();

    for one_link in all_links {
        let one_day = 86400;
        let current = time::get_time().sec;

        if one_link.time > current - one_day {
            list_of_links.push(one_link.url);
        }
    }

    return list_of_links;
}

fn write_in_array(json_stuff: &Vec<CachedUrl>) {
    let jsons = json::encode(&json_stuff).unwrap();

    let mut f = OpenOptions::new()
                    .write(true)
                    .append(false)
                    .open("bar.json").unwrap();

        f.write(jsons.as_bytes());
    }

    fn get_listings_on_page(doc: Document) -> Vec<String> {
        let mut hrefs : Vec<String> = Vec::new();

        for node in doc.find(Class("details-title")).iter() {
            let a = node.find(Name("a")).first().unwrap().attr("href").unwrap().to_string();
            hrefs.push(a);
        }
        return hrefs;
    }

    fn webber(apartments: Vec<String>, nb : & NaiveBayes) -> Vec<String> {


        let (tx, rx) = mpsc::channel();
        for apt in apartments.clone() {
            let tx = tx.clone();
            let nb2 = nb.clone();
            crossbeam::scope(|scope| {
                scope.spawn(move || {
                    let result = goGetEm(apt, nb2);
                    tx.send(result).unwrap();
                });
            });
        }
        let mut data : Vec<String> = Vec::new();
        for apt in apartments.clone() {
            let verified = rx.recv().unwrap();
            if verified == "true" {
                let url = Url::parse("http://streeteasy.com").unwrap();
                let url = url.join(&*apt).unwrap().as_str().to_string();
                data.push(url);
            }
        }

        return data;
    }

    fn goGetEm(extension: String, nb: NaiveBayes) -> String {
        let c = Client::new();
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

    fn train(request: &mut Request, atx: & Arc<Mutex<std::sync::mpsc::Sender<ApartmentUrl>>>) -> IronResult<Response> {
        let mut payload = String::new();
        request.body.read_to_string(&mut payload).unwrap();
        let request: ApartmentUrl = json::decode(&payload).unwrap();
        atx.lock().unwrap().send(request).unwrap();

        let greeting = Greeting {msg: "success".to_string()};
        let payload = json::encode(&greeting).unwrap();
        Ok(Response::with((status::Ok, payload)))
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

    let host = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 3000);
    Iron::new(router).http(host).unwrap();
    println!("On 3000");


}
