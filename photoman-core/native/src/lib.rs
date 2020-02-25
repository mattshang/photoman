extern crate neon;
extern crate hyper;
extern crate hyper_native_tls;
extern crate google_drive3;
extern crate yup_oauth2;

use neon::prelude::*;

use std::path::Path;
use std::collections::HashMap;

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
    parent: u32,
    pub is_directory: bool,
    pub children: Option<Vec<u32>>,
    image_path: Option<String>,
}

impl Entry {
    pub fn new(name: String, drive_id: String, parent: u32, is_directory: bool) -> Entry {
        Entry {
            name,
            drive_id,
            parent,
            is_directory,
            children: None,
            image_path: None,
        }
    }
}

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
        drive.entries.insert(0, Entry::new("root".to_string(), "root".to_string(), 0, true));

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
                    let name = file.name.unwrap_or(String::new());
                    let is_directory = file.mime_type.unwrap_or(String::new()) == "application/vnd.google-apps.folder";
                    self.entries.insert(new_id, Entry::new(name, drive_id, id, is_directory));

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

    pub fn get_name(&self, id: u32) -> &String {
        &self.entries.get(&id).unwrap().name
    }

    pub fn get_parent(&self, id: u32) -> u32 {
        self.entries.get(&id).unwrap().parent
    }

    // pub fn download(&self, id: u32, )
}

const CLIENT_SECRET_FILE: &'static str = "client_secret.json";

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
    }
}

register_module!(mut cx, {
    cx.export_class::<JsGoogleDrive>("GoogleDrive")?;

    Ok(())
});
