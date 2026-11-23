// The Juku emulator is its own debug adapter: it speaks DAP on 127.0.0.1
// (juku-emu's spec §12.8). This extension names the debug type, connects to it, and
// starts the app when nothing listens.
'use strict';

const vscode = require('vscode');
const fs = require('fs');
const net = require('net');
const path = require('path');
const { spawn } = require('child_process');

// How long the app may take to start listening, and how often to look.
const START_TIMEOUT_MS = 10000;
const START_POLL_MS = 250;

// Whether something accepts connections on the port.
function listening(port) {
    return new Promise((resolve) => {
        const socket = net.connect(port, '127.0.0.1');
        socket.once('connect', () => {
            socket.destroy();
            resolve(true);
        });
        socket.once('error', () => resolve(false));
    });
}

// The program to start on Windows: for the setting's default name, Juku.exe
// where the installer puts it, if it is there; otherwise the setting, a
// program on the PATH or a path.
function windowsApp(app) {
    if (app === 'Juku' && process.env.LOCALAPPDATA) {
        const installed = path.join(process.env.LOCALAPPDATA, 'Programs', 'Juku', 'Juku.exe');
        if (fs.existsSync(installed)) {
            return installed;
        }
    }
    return app;
}

// Starts the app with its debug server on: `open -a` on macOS, which takes
// an app's name or its path; elsewhere the program itself. On Windows a Juku
// already running takes the port from the new start, which then exits.
function startApp(app, port) {
    const flag = `--debug-server=${port}`;
    const [command, args] =
        process.platform === 'darwin'
            ? ['open', ['-a', app, '--args', flag]]
            : [process.platform === 'win32' ? windowsApp(app) : app, [flag]];
    spawn(command, args, { detached: true, stdio: 'ignore' }).on('error', () => {}).unref();
}

async function connect(session) {
    const settings = vscode.workspace.getConfiguration('juku');
    const port = session.configuration.port ?? settings.get('port', 5104);
    if (!(await listening(port))) {
        startApp(settings.get('app', 'Juku'), port);
        const deadline = Date.now() + START_TIMEOUT_MS;
        while (!(await listening(port))) {
            if (Date.now() > deadline) {
                throw new Error(
                    process.platform === 'win32'
                        ? `Nothing listens on port ${port}. The juku.app setting names the program to start: ` +
                              'the path of Juku.exe, if it is not where the installer puts it.'
                        : `Nothing listens on port ${port}. If Juku is running, tick Settings → Machine → Debugger: ` +
                              'Accept Connections, or quit it so that the debugger can start it. The juku.app setting ' +
                              'names the app to start.',
                );
            }
            await new Promise((resolve) => setTimeout(resolve, START_POLL_MS));
        }
    }
    return new vscode.DebugAdapterServer(port, '127.0.0.1');
}

function activate(context) {
    context.subscriptions.push(
        vscode.debug.registerDebugAdapterDescriptorFactory('juku', {
            createDebugAdapterDescriptor: connect,
        }),
        // F5 with no launch.json attaches.
        vscode.debug.registerDebugConfigurationProvider('juku', {
            resolveDebugConfiguration(folder, config) {
                if (!config.type && !config.request && !config.name) {
                    return { type: 'juku', request: 'attach', name: 'Attach to Juku' };
                }
                return config;
            },
        }),
    );
}

function deactivate() {}

module.exports = { activate, deactivate };
