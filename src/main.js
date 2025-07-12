const electron = require("electron");
const {app, BrowserWindow} = require('electron'); // to not code 'electron.app' or 'electron.BrowserWindow'


function getDisplaySize() {
    let display = electron.screen.getPrimaryDisplay()
    return {width: display.bounds.width, height: display.bounds.height};
}

const createWindow = () => {
    const windowWidth = 128;
    const windowHeight = 128;
    const win = new BrowserWindow({
		width: windowWidth,
        height: windowHeight,
        transparent: true,
        frame: false,
        alwaysOnTop: true,
        skipTaskbar: true,
        resizable: false,
        focusable: false,
    });

    let displaySize = getDisplaySize();

    win.setMenu(null);
    win.setPosition(
        displaySize.width - windowWidth,
        displaySize.height - windowHeight
    );
    win.loadFile('src/index.html');
};

app.on('ready', () => {
    createWindow();
});

// quit the app when all windows are closed (Win & Linux)
// https://www.electronjs.org/docs/latest/tutorial/tutorial-first-app#quit-the-app-when-all-windows-are-closed-windows--linux
app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') app.quit()
})

// open a window if non are open (macOS)
// https://www.electronjs.org/docs/latest/tutorial/tutorial-first-app#open-a-window-if-none-are-open-macos 
app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
})
