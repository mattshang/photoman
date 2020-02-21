extern crate neon;
extern crate hyper;
extern crate hyper_native_tls;
extern crate google_drive3;
extern crate yup_oauth2;

use neon::prelude::*;

use std::path::Path;

use hyper::net::HttpsConnector;
use hyper_native_tls::NativeTlsClient;
use hyper::client::Client;

use google_drive3::{ DriveHub };
use yup_oauth2::{
    read_application_secret, Authenticator, 
    DefaultAuthenticatorDelegate, DiskTokenStorage, FlowType,
};

pub struct GoogleDrive {
    hub: DriveHub<Client, Authenticator<
        DefaultAuthenticatorDelegate, DiskTokenStorage, Client>>,
}

impl GoogleDrive {
    pub fn new(secret_file: String) -> GoogleDrive {
        let secret = read_application_secret(Path::new(&secret_file)).unwrap();
        let client = 
            hyper::Client::with_connector(
                HttpsConnector::new(NativeTlsClient::new().unwrap()));
        let authenticator = Authenticator::new(
            &secret,
            DefaultAuthenticatorDelegate,
            client,
            DiskTokenStorage::new(&"token_store.json".to_string()).unwrap(),
            Some(FlowType::InstalledInteractive),
        );
        let client = 
            hyper::Client::with_connector(
                HttpsConnector::new(NativeTlsClient::new().unwrap()));
        let hub = DriveHub::new(client, authenticator);

        GoogleDrive { hub }
    }

    pub fn files(&self) -> Vec<String> {
        let (_resp, list_result) = self.hub
            .files()
            .list()
            .q("'root' in parents and trashed = false")
            .doit()
            .unwrap();
        
        list_result.files.unwrap_or(vec![])
            .into_iter()
            .map(|f| f.name.unwrap_or(String::new()))
            .collect()
    }
}

const CLIENT_SECRET_FILE: &'static str = "client_secret.json";

declare_types! {
    pub class JsGoogleDrive for GoogleDrive {
        init(mut cx) {
            Ok(GoogleDrive::new(CLIENT_SECRET_FILE.to_string()))
        }

        method files(mut cx) {
            let this = cx.this();
            let guard = cx.lock();

            let files = this.borrow(&guard).files();

            let js_array = JsArray::new(&mut cx, files.len() as u32);
            for (i, obj) in files.iter().enumerate() {
                let js_string = cx.string(obj);
                js_array.set(&mut cx, i as u32, js_string).unwrap();
            }

            Ok(js_array.upcast())
        }
    }
}

fn hello(mut cx: FunctionContext) -> JsResult<JsString> {
    Ok(cx.string("hello node"))
    // Ok(cx.string(bruh));
}

fn list_directory(mut cx: FunctionContext) -> JsResult<JsNumber> {
    Ok(cx.number(5 as f64))
}

register_module!(mut cx, {
    cx.export_function("hello", hello)?;
    cx.export_function("listDirectory", list_directory)?;
    cx.export_class::<JsGoogleDrive>("GoogleDrive")?;
    Ok(())
});
