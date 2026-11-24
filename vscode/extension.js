// The Juku emulator is its own debug adapter: it speaks DAP on 127.0.0.1
// (juku-emu's spec §12.8). This extension names the debug type, connects to it, and
// starts the app when nothing listens. The language server, juku-lsp, is
// started by lsp.js, and Juku: Build (build.js) runs juku-asm.
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

// After a stop in code with no source, such as a step into a RomBios
// routine, VS Code shows the first frame that has some: the call's line,
// while the CPU is elsewhere, and each step after it goes on unseen. The
// frame where the CPU is is shown instead, in the Disassembly view, as
// juku-emu's spec §12.5 means it; steps there go by instruction. VS Code
// picks its frame again as the call stack comes in, so for a moment after
// the stop the pick is undone; after that another frame can be picked.
const SETTLE_MS = 1500;

function showWhereTheCpuIs(context) {
    // Each session whose last stop's top frame has no source: when it came,
    // and whether the Disassembly view has been opened for it.
    const unseen = new Map();
    context.subscriptions.push(
        vscode.debug.registerDebugAdapterTrackerFactory('juku', {
            createDebugAdapterTracker(session) {
                let stopped = false;
                return {
                    onDidSendMessage(m) {
                        if (m.type === 'event' && (m.event === 'stopped' || m.event === 'continued')) {
                            stopped = m.event === 'stopped';
                            unseen.delete(session.id);
                        } else if (stopped && m.type === 'response' && m.command === 'stackTrace' && m.success) {
                            const top = (m.body.stackFrames || []).find((f) => f.id === 0);
                            if (top) {
                                stopped = false;
                                if (!top.source) {
                                    unseen.set(session.id, { at: Date.now(), shown: false });
                                }
                            }
                        }
                    },
                    onExit() {
                        unseen.delete(session.id);
                    },
                };
            },
        }),
        vscode.debug.onDidChangeActiveStackItem(async (item) => {
            const stop = item && item.frameId !== undefined && unseen.get(item.session.id);
            if (!stop || Date.now() - stop.at > SETTLE_MS) {
                return;
            }
            if (item.frameId > 0) {
                // Up the call stack to frame 0: the server numbers frames
                // from 0, the top, and Focus Top Stack Frame is VS Code's pick.
                for (let k = item.frameId; k > 0; k--) {
                    await vscode.commands.executeCommand('workbench.action.debug.callStackUp');
                }
            } else if (!stop.shown) {
                stop.shown = true;
                await vscode.commands.executeCommand('debug.action.openDisassemblyView');
            }
        }),
    );
}

function activate(context) {
    showWhereTheCpuIs(context);
    const client = require('./lsp').activate(context);
    require('./build').activate(context, client);
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
