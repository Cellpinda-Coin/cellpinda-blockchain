use std::sync::{Arc, Mutex};

use iron::Headers;
use iron::prelude::Response;
use iron::headers::ContentType;
use iron_test;
use router::Router;
use mime::Mime;

use serde::{Serialize, Deserialize};
use serde_json;

use exonum::api::Api;
use exonum::blockchain::Transaction;
use exonum::crypto::{hash, Hash, HexValue};
use exonum::events::Error as EventsError;
use exonum::messages::{Message, RawMessage, FromRaw};
use exonum::node::TransactionSend;
use exonum::helpers;

use timestamping::api::PublicApi;
use timestamping::{TimestampTx, Content};

use TimestampingSandbox;

pub struct TimestampingApiSandbox {
    pub router: Router,
    pub channel: TestTxSender,
}

#[derive(Debug, Default, Clone)]
pub struct TestTxSender {
    transactions: Arc<Mutex<Vec<RawMessage>>>,
}

impl TransactionSend for TestTxSender {
    fn send(&self, tx: Box<Transaction>) -> Result<(), EventsError> {
        if !tx.verify() {
            return Err(EventsError::new("Unable to verify transaction"));
        }
        let rm = tx.raw().clone();
        self.transactions.lock().unwrap().push(rm);
        Ok(())
    }
}

impl TestTxSender {
    pub fn txs(&self) -> Vec<RawMessage> {
        let mut txs = self.transactions.lock().unwrap();
        let txs = txs.drain(..);
        txs.collect::<Vec<_>>()
    }
}

fn request_put<A: AsRef<str>, B, C>(router: &Router, route: A, value: B) -> C
where
    A: AsRef<str>,
    B: Serialize,
    for<'de> C: Deserialize<'de>,
{
    let body = serde_json::to_string_pretty(&serde_json::to_value(value).unwrap()).unwrap();
    let endpoint = format!("http://127.0.0.1:8000{}", route.as_ref());

    let mut headers = Headers::new();
    let mime: Mime = "application/json".parse().unwrap();
    headers.set(ContentType(mime));

    info!("PUT request: `{}` body={}", endpoint, body);

    let response = iron_test::request::put(&endpoint, headers, &body, router).unwrap();
    serde_json::from_value(response_body(response)).unwrap()
}

fn request_get<A, B>(router: &Router, route: A) -> B
where
    A: AsRef<str>,
    for<'de> B: Deserialize<'de>,
{
    let endpoint = format!("http://127.0.0.1:8000{}", route.as_ref());

    info!("GET request: `{}`", endpoint);

    let response = iron_test::request::get(&endpoint, Headers::new(), router).unwrap();
    serde_json::from_value(response_body(response)).unwrap()
}

fn response_body(response: Response) -> serde_json::Value {
    if let Some(mut body) = response.body {
        let mut buf = Vec::new();
        body.write_body(&mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        trace!("Received response body:'{}'", &s);
        serde_json::from_str(&s).unwrap()
    } else {
        serde_json::Value::Null
    }
}

impl TimestampingApiSandbox {
    pub fn new(sandbox: &TimestampingSandbox) -> TimestampingApiSandbox {
        let mut router = Router::new();

        let channel = TestTxSender::default();
        let api = PublicApi::new(sandbox.blockchain_ref().clone(), channel.clone());
        api.wire(&mut router);

        TimestampingApiSandbox { router, channel }
    }

    pub fn put<B, C>(&self, route: &str, value: B) -> C
    where
        B: Serialize,
        for<'de> C: Deserialize<'de>,
    {
        request_put(&self.router, route, value)
    }

    pub fn get<B>(&self, route: &str) -> B
    where
        for<'de> B: Deserialize<'de>,
    {
        request_get(&self.router, route)
    }
}

#[test]
fn test_timestamping_put() {
    let _ = helpers::init_logger();

    let sandbox = TimestampingSandbox::new();

    let hash = hash(&[1, 2, 3]);
    let description = "My first hash";
    let content = Content::new(description, &hash);

    let api = TimestampingApiSandbox::new(&sandbox);

    let tx2: serde_json::Value = api.put("/v1/content", content);
    let tx = TimestampTx::from_raw(api.channel.txs()[0].clone()).unwrap();
    info!("tx2={:#?}", tx2);
    info!("tx={:#?}", serde_json::to_value(tx.clone()).unwrap());

    assert_eq!(tx2, serde_json::to_value(tx.clone()).unwrap());
    assert_eq!(serde_json::from_value::<TimestampTx>(tx2).unwrap(), tx);
}
