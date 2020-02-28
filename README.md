# photoman

Photoman is a photo management tool for Google Drive written in Electron and Rust.

## Building and Running
### Dependencies
* Electron
* Rust Toolchain
* Exiv2
* npm
* git

### Authentication
Currently, Photoman requires a `client_secret.json` token from Google Drive. The steps to acquire one:
1. Go to `console.developers.google.com`.
2. Create a new project and add the Drive SDK to the library.
3. Go to credentials and add an OAuth 2.0 client ID, setting the type to other. Press `DOWNLOAD JSON` and move the file into the root directory of Photoman. Rename it to `client_secret.json`.
4. Go to the OAuth consent screen. Add the `drive` scope and press save.

### Process
1. Clone this repository.
2. cd in and run `npm install`.
3. `npm run build`.
4. `npm start` starts the native application.