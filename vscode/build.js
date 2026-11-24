// Juku: Build: assembles the program the active file is in with juku-asm.
// The language server says which main files have the file in them
// (juku/mainFiles, docs/lsp.md); without it, an .asm file is assembled on
// its own.
'use strict';

const vscode = require('vscode');
const path = require('path');
const { execFile } = require('child_process');
const { findProgram } = require('./tools');

// Runs `program` with `args` in `cwd`: its exit code, and what it wrote.
function run(program, args, cwd) {
    return new Promise((resolve) => {
        execFile(program, args, { cwd, windowsHide: true }, (error, stdout, stderr) => {
            const code = error ? (typeof error.code === 'number' ? error.code : -1) : 0;
            resolve({ code, text: (stdout + stderr).trimEnd(), error: error && typeof error.code !== 'number' ? error.message : null });
        });
    });
}

async function build(client, output) {
    const editor = vscode.window.activeTextEditor;
    const doc = editor && editor.document;
    if (!doc || doc.languageId !== 'juku-asm' || doc.uri.scheme !== 'file') {
        vscode.window.showWarningMessage('Juku: Build builds the program of the juku-asm file being edited: open one first.');
        return;
    }
    const asm = findProgram('asm.path', 'juku-asm');
    if (!asm) {
        vscode.window.showErrorMessage(
            "juku-asm, the SDK's assembler, is not on the PATH: install the SDK's commands (build.sh install), or name it in the juku.asm.path setting.",
        );
        return;
    }
    // juku-asm reads the files from disk.
    await vscode.workspace.saveAll(false);

    let mains = null;
    try {
        mains = await client.ask('juku/mainFiles', { textDocument: { uri: doc.uri.toString() } });
    } catch (e) {
        mains = null;
    }
    const files = mains ? mains.map((u) => vscode.Uri.parse(u).fsPath) : /\.asm$/i.test(doc.fileName) ? [doc.fileName] : [];
    if (files.length === 0) {
        vscode.window.showWarningMessage(`${path.basename(doc.fileName)} is in no program to build: no main file includes it.`);
        return;
    }

    const config = vscode.workspace.getConfiguration('juku');
    const folder = vscode.workspace.getWorkspaceFolder(doc.uri);
    const base = folder ? folder.uri.fsPath : path.dirname(doc.fileName);
    const include = config.get('includePaths', []).flatMap((p) => ['-I', path.resolve(base, p)]);
    const flags = config.get('build.arguments', ['-g']);
    output.clear();
    const built = [];
    for (const file of files) {
        const args = [...flags, ...include, file];
        output.appendLine(`> juku-asm ${args.join(' ')}`);
        const result = await run(asm, args, path.dirname(file));
        if (result.text) {
            output.appendLine(result.text);
        }
        if (result.code !== 0) {
            output.appendLine(result.error ? `juku-asm (${asm}) could not be run: ${result.error}` : '');
            const first = (result.error || result.text).split('\n')[0];
            const choice = await vscode.window.showErrorMessage(`Juku: Build: ${first}`, 'Show Output');
            if (choice) {
                output.show(true);
            }
            return;
        }
        built.push(result.text.split('\n').pop());
    }
    vscode.window.setStatusBarMessage(`$(check) Built ${built.join('; ')}`, 8000);
}

function activate(context, client) {
    const output = vscode.window.createOutputChannel('Juku Build');
    context.subscriptions.push(output, vscode.commands.registerCommand('juku.build', () => build(client, output)));
}

module.exports = { activate };
