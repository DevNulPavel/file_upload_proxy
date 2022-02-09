use crate::project::Project;
use hyper::{
    body::Body as BodyStruct,
    client::connect::{dns::GaiResolver, HttpConnector},
    Client,
};
use hyper_rustls::HttpsConnector;
use std::collections::HashMap;

//////////////////////////////////////////////////////////////////////////////////////////////////////////////////

pub type HttpClient = Client<HttpsConnector<HttpConnector<GaiResolver>>, BodyStruct>;

pub struct App {
    pub projects: HashMap<String, Project>,
}
