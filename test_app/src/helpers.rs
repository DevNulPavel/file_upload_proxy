use reqwest::Url;
use std::borrow::Cow;
use serde::{de::Error, Deserialize, Deserializer};

pub fn deserialize_url<'de, D>(data: D) -> Result<Url, D::Error>
where
    D: Deserializer<'de>,
{
    let text = Cow::<str>::deserialize(data)?;
    //let text: Cow<str> = Deserialize::deserialize(data)?;

    Url::parse(&text).map_err(Error::custom)
}
