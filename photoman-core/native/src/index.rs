extern crate google_drive3;

use rusqlite::{ Connection, params };

use google_drive3::{ File };

use std::collections::HashMap;

type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

const DRIVE_FOLDER_TYPE: &'static str = "application/vnd.google-apps.folder";
const DB_PATH: &'static str = "cache/index.db";

pub struct Entry {
    name: String,
    drive_id: String,
    pub drive_type: String, 
    parent: u32,
    pub is_directory: bool,
    pub children: Option<Vec<u32>>,
    pub photo_path: Option<String>,
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
    db: Connection,
}

impl Index {
    pub fn new() -> BoxResult<Index> {
        let conn = Connection::open(DB_PATH)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS entries (
                id            INTEGER PRIMARY KEY,
                name          TEXT NOT NULL,
                drive_id      TEXT NOT NULL,
                drive_type    TEXT,
                parent        INTEGER,
                is_directory  INTEGER,
                children      TEXT,
                photo_path    TEXT
            )",
            params![],
        )?;

        let mut index = Index {
            compressed_ids: HashMap::new(),
            entries: HashMap::new(),
            db: conn,
        };
        index.restore_from_db();
        if index.is_empty() {
            // The database was empty. Initialize the index manually.
            index.create_root();
        }

        Ok(index)
    }

    fn restore_from_db(&mut self) -> BoxResult<()> {
        let entries: Vec<(u32, Entry)> = {
            let mut stmt = self.db.prepare("SELECT * FROM entries")?;
            let entry_iter = stmt.query_map(params![], |row| {
                let id = row.get(0)?;
                let mut e = Entry::new(row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?);
                let res: rusqlite::Result<String> = row.get(6);
                e.children = match res {
                    Ok(joined) => {
                        let v = joined.as_str().split(",")
                            .map(|s| s.parse::<u32>())
                            .filter_map(Result::ok)
                            .collect();
                        Some(v)
                    },
                    Err(_e) => None,
                };
                Ok((id, e))
            })?;
            let mut res: Vec<(u32, Entry)> = vec![];
            for entry_result in entry_iter {
                res.push(entry_result?)
            }
            res
        };

        for (id, e) in entries {
            self.reinsert_entry(id, e);
        }
        Ok(())
    }

    fn reinsert_entry(&mut self, id: u32, e: Entry) {
        self.compressed_ids.insert(e.drive_id.clone(), id);
        self.entries.insert(id, e);
    }

    pub fn create_entry(&mut self, e: Entry) -> BoxResult<u32> {
        let mut stmt = self.db.execute(
            "INSERT INTO entries (name, drive_id, drive_type, parent, is_directory)
                VALUES (?1, ?2, ?3, ?4, ?5)",
            params![e.name, e.drive_id, e.drive_type, e.parent, e.is_directory as u32],
        )?;
        let id = self.db.last_insert_rowid() as u32;
        self.reinsert_entry(id, e);
        Ok(id)
    }

    pub fn create_root(&mut self) {
        // The root folder is special, so manually initialize it
        let e = Entry::new("root".to_string(), "root".to_string(), DRIVE_FOLDER_TYPE.to_string(), 1, true);
        match self.create_entry(e) {
            Err(e) => eprintln!("create_entry error: {}", e),
            _ => (),
        }
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

    fn add_child(&mut self, parent_id: u32, drive_file: &File) {
        let drive_id = drive_file.id.as_ref().unwrap();
        let child_id = match self.compressed_ids.get(drive_id) {
            Some(&val) => val,
            None => {
                let name = drive_file.name.as_ref().unwrap().clone();
                let drive_type = drive_file.mime_type.as_ref().unwrap().clone();
                let is_directory = drive_type == DRIVE_FOLDER_TYPE;

                let e = Entry::new(name, drive_id.clone(), drive_type, parent_id, is_directory);
                match self.create_entry(e) {
                    Ok(id) => id,
                    Err(e) => panic!("create_entry error: {}", e),
                }
            }
        };

        let e = self.entries.get_mut(&parent_id).unwrap();
        if e.is_directory && e.children.is_some() {
            e.children.as_mut().unwrap().push(child_id);
        }
    }

    pub fn add_children(&mut self, parent_id: u32, files: &Vec<File>) -> BoxResult<()> {
        {
            let e = self.entries.get_mut(&parent_id).unwrap();
            if e.children.is_none() {
                e.children = Some(vec![]);
            }
        }

        for drive_file in files {
            self.add_child(parent_id, &drive_file);
        }

        let joined = self.get_children(parent_id)
            .into_iter()
            .map(|u| u.to_string())
            .collect::<Vec<String>>()
            .join(",");
        
        let mut stmt = self.db.prepare(
            "UPDATE entries SET children = (?1) WHERE id = (?2)"
        )?;
        stmt.execute(params![joined, parent_id])?;

        Ok(())
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

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}