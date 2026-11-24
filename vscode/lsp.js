// The language server's client: starts juku-lsp, the SDK's language server
// (docs/lsp.md), speaks LSP to it on its stdin and stdout, and gives VS
// Code what it answers: errors, going to names, hovers, outlines,
// completion, operands, renaming, folding, addresses and cycles. Written by
// hand, as the debug adapter's side is, so that the extension needs no npm
// (package.py); VS Code's own types do the rest.
'use strict';

const vscode = require('vscode');
const { spawn } = require('child_process');
const { Rpc } = require('./rpc');
const { findProgram } = require('./tools');

const SELECTOR = [{ language: 'juku-asm', scheme: 'file' }];
// How long a shutdown may take before the server is stopped.
const STOP_TIMEOUT_MS = 2000;
// A server that stops this many times in this long is not started again.
const MAX_RESTARTS = 3;
const RESTART_WINDOW_MS = 3 * 60 * 1000;
// How long after a change the addresses and cycles are asked for again:
// the server assembles once the typing has paused for 200 ms.
const HINTS_DELAY_MS = 400;

// The settings the server takes (docs/lsp.md, Settings).
function settings() {
    const c = vscode.workspace.getConfiguration('juku');
    return { mainFiles: c.get('mainFiles', []), includePaths: c.get('includePaths', []), inlayHints: c.get('inlayHints', false) };
}

// LSP's values as VS Code's.
const position = (p) => new vscode.Position(p.line, p.character);
const range = (r) => new vscode.Range(position(r.start), position(r.end));
const location = (l) => new vscode.Location(vscode.Uri.parse(l.uri), range(l.range));
// LSP's kinds count from 1, VS Code's from 0.
const symbolKind = (k) => k - 1;

function documentSymbol(s) {
    const symbol = new vscode.DocumentSymbol(s.name, s.detail || '', symbolKind(s.kind), range(s.range), range(s.selectionRange));
    symbol.children = (s.children || []).map(documentSymbol);
    return symbol;
}

class Client {
    constructor() {
        this.output = vscode.window.createOutputChannel('Juku Language Server');
        this.diagnostics = vscode.languages.createDiagnosticCollection('juku-asm');
        this.hintsChanged = new vscode.EventEmitter();
        this.rpc = null;
        this.child = null;
        this.stops = [];
        this.warned = false;
        this.hintsTimer = null;
    }

    // The server, once it has answered `initialize`.
    get running() {
        return this.rpc !== null && this.ready;
    }

    async start() {
        if (!vscode.workspace.getConfiguration('juku').get('lsp.enabled', true)) {
            return;
        }
        const exe = findProgram('lsp.path', 'juku-lsp');
        if (!exe) {
            if (!this.warned) {
                this.warned = true;
                vscode.window.showWarningMessage(
                    "juku-lsp, the SDK's language server, is not on the PATH: install the SDK's commands (build.sh install), " +
                        'or name it in the juku.lsp.path setting. Without it, .asm files have colors but no errors or names.',
                );
            }
            return;
        }
        this.ready = false;
        this.stopping = false;
        const child = spawn(exe, ['--stdio'], { stdio: ['pipe', 'pipe', 'pipe'] });
        this.child = child;
        const rpc = new Rpc((bytes) => child.stdin.write(bytes), {
            notification: (method, params) => this.notification(method, params),
            request: () => null,
        });
        this.rpc = rpc;
        child.stdout.on('data', (chunk) => rpc.receive(chunk));
        child.stderr.on('data', (chunk) => this.output.append(chunk.toString()));
        child.stdin.on('error', () => {});
        child.on('error', (e) => {
            this.output.appendLine(`juku-lsp (${exe}) could not be started: ${e.message}`);
            rpc.close(e);
        });
        child.on('exit', (code) => {
            rpc.close();
            if (this.rpc === rpc) {
                this.rpc = null;
                this.diagnostics.clear();
                if (!this.stopping) {
                    this.stopped(code);
                }
            }
        });
        const folders = (vscode.workspace.workspaceFolders || []).map((f) => ({ uri: f.uri.toString(), name: f.name }));
        try {
            await rpc.request('initialize', {
                processId: process.pid,
                clientInfo: { name: 'Juku for VS Code' },
                rootUri: folders.length ? folders[0].uri : null,
                workspaceFolders: folders.length ? folders : null,
                initializationOptions: settings(),
                capabilities: {
                    general: { positionEncodings: ['utf-16'] },
                    textDocument: { documentSymbol: { hierarchicalDocumentSymbolSupport: true } },
                },
            });
        } catch (e) {
            this.output.appendLine(`juku-lsp did not start: ${e.message}`);
            return;
        }
        rpc.notify('initialized', {});
        this.ready = true;
        for (const doc of vscode.workspace.textDocuments) {
            this.opened(doc);
        }
    }

    async stop() {
        const { rpc, child } = this;
        if (!rpc || !child) {
            return;
        }
        this.stopping = true;
        const timeout = new Promise((resolve) => setTimeout(resolve, STOP_TIMEOUT_MS));
        await Promise.race([rpc.request('shutdown', null).catch(() => {}), timeout]);
        rpc.notify('exit', null);
        setTimeout(() => child.kill(), STOP_TIMEOUT_MS);
        this.rpc = null;
        this.diagnostics.clear();
    }

    async restart() {
        await this.stop();
        await this.start();
        this.hintsChanged.fire();
    }

    // The server stopped by itself: started again, unless it keeps doing so.
    stopped(code) {
        const now = Date.now();
        this.stops = this.stops.filter((t) => now - t < RESTART_WINDOW_MS).concat([now]);
        this.output.appendLine(`juku-lsp stopped (exit code ${code}).`);
        if (this.stops.length <= MAX_RESTARTS) {
            this.start();
        } else {
            vscode.window.showErrorMessage('The Juku language server keeps stopping, and is not started again.', 'Show Output').then((choice) => {
                if (choice) {
                    this.output.show();
                }
            });
        }
    }

    notification(method, params) {
        if (method === 'textDocument/publishDiagnostics') {
            const list = params.diagnostics.map((d) => {
                const diagnostic = new vscode.Diagnostic(range(d.range), d.message, vscode.DiagnosticSeverity.Error);
                diagnostic.source = d.source;
                return diagnostic;
            });
            this.diagnostics.set(vscode.Uri.parse(params.uri), list);
        } else if (method === 'window/logMessage' || method === 'window/showMessage') {
            this.output.appendLine(params.message);
        }
    }

    // A request about `doc`, or nothing if the server is not running.
    async ask(method, params) {
        if (!this.running) {
            return null;
        }
        return this.rpc.request(method, params);
    }

    static mine(doc) {
        return doc.languageId === 'juku-asm' && doc.uri.scheme === 'file';
    }

    opened(doc) {
        if (this.running && Client.mine(doc)) {
            this.rpc.notify('textDocument/didOpen', {
                textDocument: { uri: doc.uri.toString(), languageId: 'juku-asm', version: doc.version, text: doc.getText() },
            });
        }
    }

    changed(doc) {
        if (this.running && Client.mine(doc)) {
            this.rpc.notify('textDocument/didChange', {
                textDocument: { uri: doc.uri.toString(), version: doc.version },
                contentChanges: [{ text: doc.getText() }],
            });
            clearTimeout(this.hintsTimer);
            this.hintsTimer = setTimeout(() => this.hintsChanged.fire(), HINTS_DELAY_MS);
        }
    }

    closed(doc) {
        if (this.running && Client.mine(doc)) {
            this.rpc.notify('textDocument/didClose', { textDocument: { uri: doc.uri.toString() } });
        }
    }

    // A source file changed on disk: 1 made, 2 changed, 3 deleted.
    onDisk(uri, type) {
        if (this.running) {
            this.rpc.notify('workspace/didChangeWatchedFiles', { changes: [{ uri: uri.toString(), type }] });
        }
    }

    configured() {
        if (this.running) {
            this.rpc.notify('workspace/didChangeConfiguration', { settings: { juku: settings() } });
            this.hintsChanged.fire();
        }
    }
}

// The position's request params.
const at = (doc, pos) => ({ textDocument: { uri: doc.uri.toString() }, position: { line: pos.line, character: pos.character } });
const docOnly = (doc) => ({ textDocument: { uri: doc.uri.toString() } });

function providers(client) {
    return [
        vscode.languages.registerDefinitionProvider(SELECTOR, {
            async provideDefinition(doc, pos) {
                const result = await client.ask('textDocument/definition', at(doc, pos));
                return (result || []).map(location);
            },
        }),
        vscode.languages.registerReferenceProvider(SELECTOR, {
            async provideReferences(doc, pos, context) {
                const params = { ...at(doc, pos), context: { includeDeclaration: context.includeDeclaration } };
                return ((await client.ask('textDocument/references', params)) || []).map(location);
            },
        }),
        vscode.languages.registerHoverProvider(SELECTOR, {
            async provideHover(doc, pos) {
                const result = await client.ask('textDocument/hover', at(doc, pos));
                return result ? new vscode.Hover(new vscode.MarkdownString(result.contents.value), range(result.range)) : null;
            },
        }),
        vscode.languages.registerDocumentSymbolProvider(SELECTOR, {
            async provideDocumentSymbols(doc) {
                return ((await client.ask('textDocument/documentSymbol', docOnly(doc))) || []).map(documentSymbol);
            },
        }),
        vscode.languages.registerWorkspaceSymbolProvider({
            async provideWorkspaceSymbols(query) {
                const result = (await client.ask('workspace/symbol', { query })) || [];
                return result.map((s) => new vscode.SymbolInformation(s.name, symbolKind(s.kind), s.containerName || '', location(s.location)));
            },
        }),
        vscode.languages.registerCompletionItemProvider(
            SELECTOR,
            {
                async provideCompletionItems(doc, pos) {
                    const result = (await client.ask('textDocument/completion', at(doc, pos))) || [];
                    return result.map((i) => {
                        const item = new vscode.CompletionItem(i.label, i.kind - 1);
                        item.detail = i.detail;
                        if (i.textEdit) {
                            item.range = range(i.textEdit.range);
                            item.insertText = i.textEdit.newText;
                        }
                        return item;
                    });
                },
            },
            '.',
        ),
        vscode.languages.registerSignatureHelpProvider(
            SELECTOR,
            {
                async provideSignatureHelp(doc, pos) {
                    const result = await client.ask('textDocument/signatureHelp', at(doc, pos));
                    if (!result) {
                        return null;
                    }
                    const help = new vscode.SignatureHelp();
                    help.signatures = result.signatures.map((s) => {
                        const doc = s.documentation ? new vscode.MarkdownString(s.documentation.value) : undefined;
                        const info = new vscode.SignatureInformation(s.label, doc);
                        info.parameters = s.parameters.map((p) => new vscode.ParameterInformation(p.label));
                        return info;
                    });
                    help.activeSignature = result.activeSignature;
                    help.activeParameter = result.activeParameter;
                    return help;
                },
            },
            ' ',
            ',',
        ),
        vscode.languages.registerRenameProvider(SELECTOR, {
            async prepareRename(doc, pos) {
                const result = await client.ask('textDocument/prepareRename', at(doc, pos));
                if (!result) {
                    throw new Error('No name to rename here.');
                }
                return { range: range(result.range), placeholder: result.placeholder };
            },
            async provideRenameEdits(doc, pos, newName) {
                const result = await client.ask('textDocument/rename', { ...at(doc, pos), newName });
                const edit = new vscode.WorkspaceEdit();
                for (const [uri, edits] of Object.entries((result && result.changes) || {})) {
                    for (const e of edits) {
                        edit.replace(vscode.Uri.parse(uri), range(e.range), e.newText);
                    }
                }
                return edit;
            },
        }),
        vscode.languages.registerFoldingRangeProvider(SELECTOR, {
            async provideFoldingRanges(doc) {
                const result = (await client.ask('textDocument/foldingRange', docOnly(doc))) || [];
                return result.map((r) => {
                    const kind = r.kind === 'comment' ? vscode.FoldingRangeKind.Comment : vscode.FoldingRangeKind.Region;
                    return new vscode.FoldingRange(r.startLine, r.endLine, kind);
                });
            },
        }),
        vscode.languages.registerInlayHintsProvider(SELECTOR, {
            onDidChangeInlayHints: client.hintsChanged.event,
            async provideInlayHints(doc, r) {
                const params = { ...docOnly(doc), range: { start: { line: r.start.line, character: 0 }, end: { line: r.end.line, character: 0 } } };
                const result = (await client.ask('textDocument/inlayHint', params)) || [];
                return result.map((h) => {
                    const hint = new vscode.InlayHint(position(h.position), h.label);
                    hint.paddingLeft = !!h.paddingLeft;
                    return hint;
                });
            },
        }),
    ];
}

function activate(context) {
    const client = new Client();
    const watcher = vscode.workspace.createFileSystemWatcher('**/*.{asm,inc}');
    context.subscriptions.push(
        client.output,
        client.diagnostics,
        watcher,
        watcher.onDidCreate((uri) => client.onDisk(uri, 1)),
        watcher.onDidChange((uri) => client.onDisk(uri, 2)),
        watcher.onDidDelete((uri) => client.onDisk(uri, 3)),
        vscode.workspace.onDidOpenTextDocument((doc) => client.opened(doc)),
        vscode.workspace.onDidChangeTextDocument((e) => client.changed(e.document)),
        vscode.workspace.onDidCloseTextDocument((doc) => client.closed(doc)),
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('juku.lsp')) {
                client.restart();
            } else if (e.affectsConfiguration('juku')) {
                client.configured();
            }
        }),
        vscode.workspace.onDidChangeWorkspaceFolders(() => client.restart()),
        vscode.commands.registerCommand('juku.restartLanguageServer', () => client.restart()),
        ...providers(client),
        { dispose: () => client.stop() },
    );
    client.start();
    return client;
}

module.exports = { activate };
