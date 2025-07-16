const {app, BrowserWindow, screen, ipcMain} = require('electron');
const activeWin = require('active-win');

let win = null;

function getDisplaySize() {
    const display = screen.getPrimaryDisplay();
    return {width: display.bounds.width, height: display.bounds.height};
}

function createWindow() {
    const windowWidth = 128;
    const windowHeight = 128;
    win = new BrowserWindow({
        width: windowWidth,
        height: windowHeight,
        transparent: true,
        frame: false,
        alwaysOnTop: true,
        skipTaskbar: true,
        resizable: false,
        focusable: false,
        webPreferences: {
            preload: __dirname + '/preload.js',
            contextIsolation: true,
        },
    });

    const displaySize = getDisplaySize();

    win.setMenu(null);
    win.setPosition(
        displaySize.width - windowWidth,
        displaySize.height - windowHeight
    );
    win.loadFile('src/index.html');
    win.on('closed', () => {
        win = null;
    });

    let lastIDE = false;
    setInterval(async () => {
        try {
            const res = await activeWin();
            const ideList = [
                'Code', 'pycharm', 'idea',
                'webstorm', 'clion', 'goland', 'rustrover'
            ];
            const isIDE = !!(
                res &&
                ideList.some(n =>
                    res.owner && res.owner.name && res.owner.name.toLowerCase().includes(n.toLowerCase())
                )
            );
            if (isIDE !== lastIDE) {
                lastIDE = isIDE;
                if (win && win.webContents) {
                    win.webContents.send('ide-mode', isIDE);
                    console.log('[main] send ide-mode', isIDE);
                }
            }
        } catch (e) {
            // skip
        }
    }, 500); // every 0.5 sec
}

// IPC: обработка запроса на выход
ipcMain.on('close-dummy-app', () => {
    app.quit();
});

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') app.quit();
});

app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
});