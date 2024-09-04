let panicImpl;
let logImpl;

function makeLogFunction(level) {
    return (length, pMessage) => {
        logImpl(level, length, pMessage);
    };
}

let { instance: hakuInstance, module: hakuModule } = await WebAssembly.instantiateStreaming(
    fetch(HAKU_WASM_PATH),
    {
        env: {
            panic(length, pMessage) {
                panicImpl(length, pMessage);
            },
            trace: makeLogFunction("trace"),
            debug: makeLogFunction("debug"),
            info: makeLogFunction("info"),
            warn: makeLogFunction("warn"),
            error: makeLogFunction("error"),
        },
    },
);

let memory = hakuInstance.exports.memory;
let w = hakuInstance.exports;

let textEncoder = new TextEncoder();
function allocString(string) {
    let size = string.length * 3;
    let align = 1;
    let pString = w.haku_alloc(size, align);

    let buffer = new Uint8Array(memory.buffer, pString, size);
    let result = textEncoder.encodeInto(string, buffer);

    return {
        ptr: pString,
        length: result.written,
        size,
        align,
    };
}

function freeString(alloc) {
    w.haku_free(alloc.ptr, alloc.size, alloc.align);
}

let textDecoder = new TextDecoder();
function readString(size, pString) {
    let buffer = new Uint8Array(memory.buffer, pString, size);
    return textDecoder.decode(buffer);
}

function readCString(pCString) {
    let memoryBuffer = new Uint8Array(memory.buffer);

    let pCursor = pCString;
    while (memoryBuffer[pCursor] != 0 && memoryBuffer[pCursor] != null) {
        pCursor++;
    }

    let size = pCursor - pCString;
    return readString(size, pCString);
}

class Panic extends Error {
    name = "Panic";
}

panicImpl = (length, pMessage) => {
    throw new Panic(readString(length, pMessage));
};

logImpl = (level, length, pMessage) => {
    console[level](readString(length, pMessage));
};

w.haku_init_logging();

export class Pixmap {
    #pPixmap = 0;

    constructor(width, height) {
        this.#pPixmap = w.haku_pixmap_new(width, height);
        this.width = width;
        this.height = height;
    }

    destroy() {
        w.haku_pixmap_destroy(this.#pPixmap);
    }

    clear(r, g, b, a) {
        w.haku_pixmap_clear(this.#pPixmap, r, g, b, a);
    }

    get ptr() {
        return this.#pPixmap;
    }

    getArrayBuffer() {
        return new Uint8ClampedArray(
            memory.buffer,
            w.haku_pixmap_data(this.#pPixmap),
            this.width * this.height * 4,
        );
    }

    getImageData() {
        return new ImageData(this.getArrayBuffer(), this.width, this.height);
    }
}

export class Haku {
    #pInstance = 0;
    #pBrush = 0;
    #brushCode = null;

    constructor(limits) {
        console.groupCollapsed("construct Haku");

        let pLimits = w.haku_limits_new();
        for (let name of Object.keys(limits)) {
            w[`haku_limits_set_${name}`](pLimits, limits[name]);
        }

        this.#pInstance = w.haku_instance_new(pLimits);
        this.#pBrush = w.haku_brush_new();

        w.haku_limits_destroy(pLimits);

        console.groupEnd();
    }

    destroy() {
        w.haku_brush_destroy(this.#pBrush);
        w.haku_instance_destroy(this.#pInstance);
    }

    setBrush(code) {
        w.haku_reset(this.#pInstance);
        // NOTE: Brush is invalid at this point, because we reset removes all defs and registered chunks.

        if (this.#brushCode != null) freeString(this.#brushCode);
        this.#brushCode = allocString(code);

        let statusCode = w.haku_compile_brush(
            this.#pInstance,
            this.#pBrush,
            this.#brushCode.length,
            this.#brushCode.ptr,
        );
        if (!w.haku_is_ok(statusCode)) {
            if (w.haku_is_diagnostics_emitted(statusCode)) {
                let diagnostics = [];
                for (let i = 0; i < w.haku_num_diagnostics(this.#pBrush); ++i) {
                    diagnostics.push({
                        start: w.haku_diagnostic_start(this.#pBrush, i),
                        end: w.haku_diagnostic_end(this.#pBrush, i),
                        message: readString(
                            w.haku_diagnostic_message_len(this.#pBrush, i),
                            w.haku_diagnostic_message(this.#pBrush, i),
                        ),
                    });
                }
                return {
                    status: "error",
                    errorKind: "diagnostics",
                    diagnostics,
                };
            } else {
                return {
                    status: "error",
                    errorKind: "plain",
                    message: readCString(w.haku_status_string(statusCode)),
                };
            }
        }

        return { status: "ok" };
    }

    #statusCodeToResultObject(statusCode) {
        if (!w.haku_is_ok(statusCode)) {
            if (w.haku_is_exception(statusCode)) {
                return {
                    status: "error",
                    errorKind: "exception",
                    description: readCString(w.haku_status_string(statusCode)),
                    message: readString(
                        w.haku_exception_message_len(this.#pInstance),
                        w.haku_exception_message(this.#pInstance),
                    ),
                };
            } else {
                return {
                    status: "error",
                    errorKind: "plain",
                    message: readCString(w.haku_status_string(statusCode)),
                };
            }
        } else {
            return { status: "ok" };
        }
    }

    evalBrush() {
        return this.#statusCodeToResultObject(w.haku_eval_brush(this.#pInstance, this.#pBrush));
    }

    renderValue(pixmap, translationX, translationY) {
        return this.#statusCodeToResultObject(
            w.haku_render_value(this.#pInstance, pixmap.ptr, translationX, translationY),
        );
    }

    resetVm() {
        w.haku_reset_vm(this.#pInstance);
    }
}
