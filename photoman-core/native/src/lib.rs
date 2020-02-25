extern crate neon;
extern crate hyper;
extern crate hyper_native_tls;
extern crate google_drive3;
extern crate yup_oauth2;

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

pub struct Entry {
    name: String,
    drive_id: String,
    pub drive_type: String, 
    parent: u32,
    pub is_directory: bool,
    pub children: Option<Vec<u32>>,
    photo_path: Option<String>,
}

impl Entry {
    pub fn new(name: String, drive_id: String, drive_type: String, parent: u32, is_directory: bool) -> Entry {
        Entry {
            name,
            drive_id,
            drive_type,
            parent,
            is_directory,
            children: None,
            photo_path: None,
        }
    }
}

const DRIVE_FOLDER_TYPE: &'static str = "application/vnd.google-apps.folder";

pub struct GoogleDrive {
    hub: DriveHub<Client, Authenticator<
        DefaultAuthenticatorDelegate, DiskTokenStorage, Client>>,
    compressed_ids: HashMap<String, u32>,
    entries: HashMap<u32, Entry>,
    current_id: u32,
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
            compressed_ids: HashMap::new(),
            entries: HashMap::new(),
            current_id: 1
        };

        // The root folder is special, so manually initialize it
        drive.compressed_ids.insert("root".to_string(), 0);
        drive.entries.insert(0, Entry::new("root".to_string(), "root".to_string(), DRIVE_FOLDER_TYPE.to_string(), 0, true));

        drive
    }

    // Returns Vec with the ids of children of the folder represented by
    // the input id. 
    pub fn get_children(&mut self, id: u32) -> Vec<u32> {
        // Start with an immutable reference, since inserting into self.entries
        // requires a mutable reference to self.entries
        let entry = self.entries.get(&id).unwrap();
        if !entry.is_directory {
            panic!("Tried to call get_children on a non-directory.");
        }

        if entry.children.is_some() {
            return entry.children.as_ref().unwrap().clone();
        }

        // entry's children have not been loaded yet. Load them now.
        let drive_id = &entry.drive_id;
        let query = format!("'{}' in parents and trashed = false", drive_id);
        // Get Vec<google_drive3::File> list_result
        let (_resp, list_result) = self.hub
            .files()
            .list()
            .q(&query)
            .doit()
            .unwrap();

        let mut children: Vec<u32> = vec![];
        for file in list_result.files.unwrap_or(vec![]) {
            let drive_id = file.id.unwrap_or(String::new());

            // Has the child already been seen?
            let child_id = match self.compressed_ids.get(&drive_id) {
                Some(&val) => val,
                None => {
                    // No, this child hasn't been indexed yet.
                    let new_id = self.current_id;
                    // Consume this current_id
                    self.current_id += 1;
                    // Add this child to the index
                    self.compressed_ids.insert(drive_id.clone(), new_id);
                    let name = file.name.unwrap_or(String::new()).clone();
                    let drive_type = file.mime_type.unwrap_or(String::new()).clone();
                    let is_directory = drive_type == DRIVE_FOLDER_TYPE;
                    self.entries.insert(new_id, Entry::new(name, drive_id, drive_type, id, is_directory));

                    new_id
                }
            };
            children.push(child_id);
        }

        // Now, get a mutable reference to entry in order to modify it
        let entry: &mut Entry = self.entries.get_mut(&id).unwrap();
        // Keep a cloned version owned by this function to return
        let clone = children.clone();
        entry.children = Some(children);

        clone
    }

    // Returns path to photo on disk. 
    // If the photo is already downloaded, it directly returns the path. Otherwise,
    // the photo is downloaded to the local cache and the path returned.
    pub fn get_photo_path(&mut self, id: u32) -> Result<String, io::Error> {
        let entry = self.entries.get(&id).unwrap();
        if entry.is_directory {
            panic!("Tried to call get_photo_path on a non-photo.");
        }

        if entry.photo_path.is_some() {
            return Ok(entry.photo_path.as_ref().unwrap().clone());
        }

        // Download the photo from Google Drive
        let scope = "https://www.googleapis.com/auth/drive";
        let (mut resp, _file) = self.hub
            .files()     
            .get(&entry.drive_id)
            .param("alt", "media")
            // .param("fields", "thumbnailLink")
            .add_scope(scope)
            .doit()
            .unwrap();
        
        let extension = Path::new(&entry.name)
            .extension()
            .and_then(OsStr::to_str)
            .unwrap();
        let mut path = format!("cache/{}.{}", id, extension);
        let mut out = fs::File::create(&path)?;
        // Write HTTPS response to file on disk
        io::copy(&mut resp, &mut out).expect("failed to write photo to local disk");

        // Instead of having to convert the RAW image to JPG ourselves,
        // NEF RAW files include their own headers with a preview JPG
        // already created. This uses exiv2 to extract the included preview
        // to a separate file.
        if entry.drive_type == "image/x-nikon-nef" {
            Command::new("/usr/local/bin/exiv2")
                .args(&["-ep3", "-l", "./cache/", &path])
                .status()
                .expect("failed to execute exiv2");
            let preview = format!("cache/{}-preview3.jpg", id);
            path = format!("cache/{}.jpg", id);
            fs::rename(&preview, &path)?;
        }

        let entry = self.entries.get_mut(&id).unwrap();
        entry.photo_path = Some(path.clone());

        Ok(path)
    }

    pub fn get_name(&self, id: u32) -> &String {
        &self.entries.get(&id).unwrap().name
    }

    pub fn get_parent(&self, id: u32) -> u32 {
        self.entries.get(&id).unwrap().parent
    }

    pub fn is_directory(&self, id: u32) -> bool {
        self.entries.get(&id).unwrap().is_directory
    }

    // pub fn is_loaded(&self, id: u32) -> bool {
    //     if self.is_directory(id) {
    //         self.entries.get(&id).unwrap().children.is_some()
    //     } else {
    //         self.entries.get(&id).unwrap().photo_path.is_some()
    //     }
    // }
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
            let name: String = cx.borrow(&this, |drive| drive.get_name(id).clone());
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
