// preload.js
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
    closeApp: () => ipcRenderer.send('close-dummy-app'),
    onIDEStatus: (callback) => ipcRenderer.on('ide-mode', (_, isIDE) => callback(isIDE)),
});