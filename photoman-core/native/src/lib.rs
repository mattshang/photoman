extern crate neon;
extern crate hyper;
extern crate hyper_native_tls;
extern crate google_drive3;
extern crate yup_oauth2;
extern crate rusqlite;

use neon::prelude::*;

use std::fs;
use std::io;
use std::path::Path;
use std::collections::HashMap;
use std::process::Command;
use std::ffi::OsStr;

use hyper::net::HttpsConnector;
use hyper_native_tls::NativeTlsClient;
use hyper::client::Client;

use google_drive3::{ DriveHub };
use yup_oauth2::{
    read_application_secret, Authenticator, 
    DefaultAuthenticatorDelegate, DiskTokenStorage, FlowType,
};

mod index;

pub struct GoogleDrive {
    hub: DriveHub<Client, Authenticator<
        DefaultAuthenticatorDelegate, DiskTokenStorage, Client>>,
    index: index::Index,
}

impl GoogleDrive {
    pub fn new(secret_file: String) -> GoogleDrive {
        // Connect to Google Drive API
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

        let mut drive = GoogleDrive { 
            hub: hub,
            index: index::Index::new(),
        };

        drive
    }

    // Returns Vec with the ids of children of the folder represented by
    // the input id. 
    pub fn get_children(&mut self, id: u32) -> Vec<u32> {
        if !self.index.is_directory(id) {
            panic!("Tried to call get_children on a non-directory.");
        }
        // Early exit if the directory is already fully loaded
        if self.index.is_fully_loaded(id) {
            return self.index.get_children(id);
        }

        // entry's children have not been loaded yet. Load them now.
        let drive_id = self.index.get_drive_id(id);
        let query = format!("'{}' in parents and trashed = false", drive_id);
        // Get Vec<google_drive3::File> list_result
        let (_resp, list_result) = self.hub
            .files()
            .list()
            .q(&query)
            .doit()
            .unwrap();

        // let mut children: Vec<u32> = vec![];
        for file in list_result.files.unwrap_or(vec![]) {
            self.index.add_child(id, &file);
        }

        // clone
        self.index.get_children(id)
    }

    // Returns path to photo on disk. 
    // If the photo is already downloaded, it directly returns the path. Otherwise,
    // the photo is downloaded to the local cache and the path returned.
    pub fn get_photo_path(&mut self, id: u32) -> Result<String, io::Error> {
        if self.index.is_fully_loaded(id) {
            return Ok(self.index.get_photo_path(id).to_string());
        }

        use std::time::Instant;
        let now = Instant::now();
        // Download the photo from Google Drive
        let scope = "https://www.googleapis.com/auth/drive";
        let drive_id = self.index.get_drive_id(id);
        let (mut resp, _file) = self.hub
            .files()     
            .get(drive_id)
            .param("alt", "media")
            // .param("fields", "thumbnailLink")
            .add_scope(scope)
            .doit()
            .unwrap();

        let elapsed = now.elapsed();
        println!("request took: {:.2?}", elapsed);
        let now = Instant::now();
        
        let extension = Path::new(self.index.get_name(id))
            .extension()
            .and_then(OsStr::to_str)
            .unwrap();
        let mut path = format!("cache/{}.{}", id, extension);
        let mut out = fs::File::create(&path)?;
        // Write HTTPS response to file on disk
        io::copy(&mut resp, &mut out).expect("failed to write photo to local disk");

        let elapsed = now.elapsed();
        println!("write took: {:.2?}", elapsed);
        let now = Instant::now();

        // Instead of having to convert the RAW image to JPG ourselves,
        // NEF RAW files include their own headers with a preview JPG
        // already created. This uses exiv2 to extract the included preview
        // to a separate file.
        if self.index.get_drive_type(id) == "image/x-nikon-nef" {
            Command::new("/usr/local/bin/exiv2")
                .args(&["-ep3", "-l", "./cache/", &path])
                .status()
                .expect("failed to execute exiv2");
            let preview = format!("cache/{}-preview3.jpg", id);
            path = format!("cache/{}.jpg", id);
            fs::rename(&preview, &path)?;
        }

        let elapsed = now.elapsed();
        println!("convert took: {:.2?}", elapsed);
        let now = Instant::now();

        self.index.add_loaded_photo(id, &path);

        Ok(path)
    }

    pub fn get_name(&self, id: u32) -> String {
        self.index.get_name(id).clone().to_string()
    }

    pub fn get_parent(&self, id: u32) -> u32 {
        self.index.get_parent(id)
    }

    pub fn is_directory(&self, id: u32) -> bool {
        self.index.is_directory(id)
    }
}

const CLIENT_SECRET_FILE: &'static str = "client_secret.json";

// struct LoadPhotoTask {
//     id: u32,
// }

// impl Task for LoadPhotoTask {
//     type Output = String;
//     type Error = String;
//     type JsEvent = JsString;

//     fn perform(&self) -> Result<String, String> {

//     }
// }

declare_types! {
    pub class JsGoogleDrive for GoogleDrive {
        init(mut cx) {
            Ok(GoogleDrive::new(CLIENT_SECRET_FILE.to_string()))
        }

        method getChildren(mut cx) {
            let id: u32 = cx.argument::<JsNumber>(0)?.value() as u32;

            let mut this = cx.this();
            let children: Vec<u32> = cx.borrow_mut(&mut this, |mut drive| {
                drive.get_children(id)
            });

            let js_array = JsArray::new(&mut cx, children.len() as u32);
            for (i, &obj) in children.iter().enumerate() {
                let js_num = cx.number(obj as f64);
                js_array.set(&mut cx, i as u32, js_num).unwrap();
            }
            Ok(js_array.upcast())
        }

        method getPhotoPath(mut cx) {
            let id: u32 = cx.argument::<JsNumber>(0)?.value() as u32;

            use std::time::Instant;
            let now = Instant::now();

            let mut this = cx.this();
            let path: String = cx.borrow_mut(&mut this, |mut drive| {
                match drive.get_photo_path(id) {
                    Ok(path) => path,
                    Err(e) => panic!("getPhotoPath threw an error")
                }
            });

            let elapsed = now.elapsed();
            println!("getPhotoPath took: {:.2?}", elapsed);

            Ok(cx.string(path).upcast())
        }

        method getName(mut cx) {
            let id: u32 = cx.argument::<JsNumber>(0)?.value() as u32;
            let this = cx.this();
            let name: String = cx.borrow(&this, |drive| drive.get_name(id));
            Ok(cx.string(name).upcast())
        }

        method getParent(mut cx) {
            let id: u32 = cx.argument::<JsNumber>(0)?.value() as u32;
            let this = cx.this();
            let par: u32 = cx.borrow(&this, |drive| drive.get_parent(id));
            Ok(cx.number(par as f64).upcast())
        }

        method isDirectory(mut cx) {
            let id: u32 = cx.argument::<JsNumber>(0)?.value() as u32;
            let this = cx.this();
            let is_directory: bool = cx.borrow(&this, |drive| drive.is_directory(id));
            Ok(cx.boolean(is_directory).upcast())
        }
    }
}

register_module!(mut cx, {
    cx.export_class::<JsGoogleDrive>("GoogleDrive")?;

    Ok(())
});
