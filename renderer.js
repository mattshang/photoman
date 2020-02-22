const ipc = require('electron').ipcRenderer;

const backButton = document.getElementById('back-button');
backButton.addEventListener('click', e => {
  ipc.send('file-back');
})

const fileList = document.getElementById('file-list');
fileList.addEventListener('click', e => {
  // Make sure this was actually triggered by an <li>
  if (e.target.tagName === 'LI') {
    // dataset.id is an HTML5 custom attribute
    const id = parseInt(e.target.dataset.id);
    ipc.send('request-files', id);
  }
});

ipc.on('load-files', (event, arg) => {
  // Remove all <li> from the <ul>
  while (fileList.firstChild) {
    fileList.removeChild(fileList.firstChild);
  }

  // Append new <li>
  for (let [id, name] of arg) {
    const li = document.createElement('li');
    li.appendChild(document.createTextNode(name));
    // Use HTML5 custom attributes
    li.setAttribute("data-id", id);
    fileList.appendChild(li);
  }
});