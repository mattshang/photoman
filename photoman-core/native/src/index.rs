extern crate google_drive3;

use rusqlite::{ Connection, Result };
use rusqlite::NO_PARAMS;

use google_drive3::{ File };

use std::collections::HashMap;

const DRIVE_FOLDER_TYPE: &'static str = "application/vnd.google-apps.folder";

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

pub struct Index {
    compressed_ids: HashMap<String, u32>,
    entries: HashMap<u32, Entry>,
    current_id: u32,
}

impl Index {
    pub fn new() -> Index {
        let mut index = Index {
            compressed_ids: HashMap::new(),
            entries: HashMap::new(),
            current_id: 1,
        };
        index.create_root();

        index
    }

    pub fn create_root(&mut self) {
        // The root folder is special, so manually initialize it
        self.compressed_ids.insert("root".to_string(), 0);
        self.entries.insert(0, Entry::new("root".to_string(), "root".to_string(), DRIVE_FOLDER_TYPE.to_string(), 0, true));
    }

    pub fn is_fully_loaded(&self, id: u32) -> bool {
        let e = self.entries.get(&id).unwrap();
        if e.is_directory {
            e.children.is_some()
        } else {
            e.photo_path.is_some()
        }
    }

    pub fn load_directory(&mut self, id: u32) {
        if !self.is_fully_loaded(id) {
            let e = self.entries.get_mut(&id).unwrap();
            if e.is_directory {
                e.children = Some(vec![]);
            }
        }
    }

    pub fn add_child(&mut self, id: u32, drive_file: &File) {
        {
            let e = self.entries.get_mut(&id).unwrap();
            if e.children.is_none() {
                e.children = Some(vec![]);
            }
        }

        // let drive_id = drive_file.id.as_ref().unwrap_or(&String::new());
        let drive_id = drive_file.id.as_ref().unwrap();
        let child_id = match self.compressed_ids.get(drive_id) {
            Some(&val) => val,
            None => {
                let new_id = self.current_id;
                self.current_id += 1;
                self.compressed_ids.insert(drive_id.clone(), new_id);
                let name = drive_file.name.as_ref().unwrap().clone();
                let drive_type = drive_file.mime_type.as_ref().unwrap().clone();
                let is_directory = drive_type == DRIVE_FOLDER_TYPE;
                self.entries.insert(new_id, Entry::new(name, drive_id.clone(), drive_type, id, is_directory));

                new_id
            }
        };

        {
            let e = self.entries.get_mut(&id).unwrap();
            if e.is_directory && e.children.is_some() {
                e.children.as_mut().unwrap().push(child_id);
            }
        }
    }
    
    pub fn get_children(&self, id: u32) -> Vec<u32> {
        let e = self.entries.get(&id).unwrap();
        if e.is_directory {
            e.children.as_ref().unwrap().clone()
        } else {
            vec![]
        }
    }

    pub fn add_loaded_photo(&mut self, id: u32, path: &str) {
        let e = self.entries.get_mut(&id).unwrap();
        e.photo_path = Some(path.clone().to_string());
    }

    pub fn get_photo_path(&self, id: u32) -> String {
        self.entries.get(&id).unwrap().photo_path.as_ref().unwrap().clone()
    }

    // pub fn get_photo_path(&self, id: u32) -> 

    pub fn get_drive_id(&self, id: u32) -> &str {
        &self.entries.get(&id).unwrap().drive_id
    }

    pub fn get_name(&self, id: u32) -> &str {
        &self.entries.get(&id).unwrap().name
    }

    pub fn get_parent(&self, id: u32) -> u32 {
        self.entries.get(&id).unwrap().parent
    }

    pub fn get_drive_type(&self, id: u32) -> &str {
        &self.entries.get(&id).unwrap().drive_type
    }

    pub fn is_directory(&self, id: u32) -> bool {
        self.entries.get(&id).unwrap().is_directory
    }
}