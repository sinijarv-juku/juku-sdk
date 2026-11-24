// Finding the SDK's commands.
'use strict';

const vscode = require('vscode');
const fs = require('fs');
const os = require('os');
const path = require('path');

// The SDK's command `name` (juku-lsp, juku-asm): the path the setting
// `juku.<setting>` gives, else the one on the PATH, else the one in cargo's
// bin folder, where the SDK's `build.sh install` puts it. Null if none.
function findProgram(setting, name) {
    const given = vscode.workspace.getConfiguration('juku').get(setting, '');
    if (given) {
        return given;
    }
    const exe = process.platform === 'win32' ? `${name}.exe` : name;
    const cargo = process.env.CARGO_HOME || path.join(os.homedir(), '.cargo');
    const dirs = (process.env.PATH || '').split(path.delimiter).concat([path.join(cargo, 'bin')]);
    for (const dir of dirs) {
        const candidate = path.join(dir, exe);
        if (dir && fs.existsSync(candidate)) {
            return candidate;
        }
    }
    return null;
}

module.exports = { findProgram };
