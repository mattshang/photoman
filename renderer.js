const { GoogleDrive } = require('photoman-core');
const ipc = require('electron').ipcRenderer;

const fileList = document.getElementById('file-list');
ipc.on('loadFiles', (event, arg) => {
  console.log(arg);
  for (let name of arg) {
    const li = document.createElement('li');
    li.appendChild(document.createTextNode(name));
    fileList.appendChild(li);
  }
});