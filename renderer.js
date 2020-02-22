const ipc = require('electron').ipcRenderer;

const backButton = document.getElementById('back-button');
backButton.addEventListener('click', e => {
  ipc.send('file-back');
})

const fileList = document.querySelector('#file-table tbody');
fileList.addEventListener('click', e => {
  // make sure we get the enclosing <tr>
  var target = e.target;
  while (target.tagName !== 'TR' && target !== fileList) {
    target = target.parentNode;
  }

  // dataset.id is an HTML5 custom attribute
  const id = parseInt(target.dataset.id);
  ipc.send('request-files', id);
});

ipc.on('load-files', (event, arg) => {
  // Remove all <tr> from the <tbody>
  while (fileList.firstChild) {
    fileList.removeChild(fileList.firstChild);
  }

  for (let [id, name] of arg) {
    // Append new <tr>
    var row = document.createElement('tr');
    var nameCell = document.createElement('td');
    nameCell.appendChild(document.createTextNode(name));
    row.appendChild(nameCell);
    var typeCell = document.createElement('td');
    typeCell.appendChild(document.createTextNode('something'));
    row.appendChild(typeCell);
    var sizeCell = document.createElement('td');
    sizeCell.appendChild(document.createTextNode('big'));
    row.appendChild(sizeCell);

    // Use HTML5 custom attribute to store id
    row.setAttribute('data-id', id);
    fileList.appendChild(row);
  }
});