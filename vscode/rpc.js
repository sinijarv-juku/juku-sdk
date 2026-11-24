// JSON-RPC as LSP frames it: each message a `Content-Length` header and a
// JSON body. Requests are answered by id; notifications are not. It needs
// no vscode, so it can be tried with node alone.
'use strict';

class Rpc {
    // `write` sends bytes; `handlers.notification(method, params)` takes
    // the other side's notifications, `handlers.request(method, params)`
    // answers its requests (a value, or a promise of one).
    constructor(write, handlers) {
        this.write = write;
        this.handlers = handlers;
        this.input = Buffer.alloc(0);
        this.nextId = 1;
        this.pending = new Map();
        this.closed = null;
    }

    // Sends request `method`; the promise is its result, or rejects with an
    // Error carrying the response's `code`.
    request(method, params) {
        if (this.closed) {
            return Promise.reject(this.closed);
        }
        const id = this.nextId++;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            this.send({ jsonrpc: '2.0', id, method, params });
        });
    }

    notify(method, params) {
        if (!this.closed) {
            this.send({ jsonrpc: '2.0', method, params });
        }
    }

    send(message) {
        const body = Buffer.from(JSON.stringify(message), 'utf8');
        this.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, 'ascii'), body]));
    }

    // Takes bytes the other side sent.
    receive(chunk) {
        this.input = Buffer.concat([this.input, chunk]);
        for (;;) {
            const end = this.input.indexOf('\r\n\r\n');
            if (end < 0) {
                return;
            }
            const header = this.input.subarray(0, end).toString('ascii');
            const found = /content-length:\s*(\d+)/i.exec(header);
            if (!found) {
                this.close(new Error('a message without Content-Length'));
                return;
            }
            const length = Number(found[1]);
            if (this.input.length < end + 4 + length) {
                return;
            }
            const body = this.input.subarray(end + 4, end + 4 + length).toString('utf8');
            this.input = this.input.subarray(end + 4 + length);
            let message;
            try {
                message = JSON.parse(body);
            } catch (e) {
                continue;
            }
            this.dispatch(message);
        }
    }

    dispatch(message) {
        if (message.method === undefined) {
            const waiting = this.pending.get(message.id);
            if (waiting) {
                this.pending.delete(message.id);
                if (message.error) {
                    const e = new Error(message.error.message);
                    e.code = message.error.code;
                    waiting.reject(e);
                } else {
                    waiting.resolve(message.result);
                }
            }
        } else if (message.id === undefined) {
            this.handlers.notification(message.method, message.params);
        } else {
            Promise.resolve()
                .then(() => this.handlers.request(message.method, message.params))
                .then(
                    (result) => this.send({ jsonrpc: '2.0', id: message.id, result: result === undefined ? null : result }),
                    (e) => this.send({ jsonrpc: '2.0', id: message.id, error: { code: -32603, message: String(e && e.message) } }),
                );
        }
    }

    // The other side is gone: every request waiting fails with `error`.
    close(error) {
        this.closed = error || new Error('the language server has stopped');
        for (const { reject } of this.pending.values()) {
            reject(this.closed);
        }
        this.pending.clear();
    }
}

module.exports = { Rpc };
