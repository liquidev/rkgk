let panicImpl;
let logImpl;

function makeLogFunction(level) {
    return (length, pMessage) => {
        logImpl(level, length, pMessage);
    };
}

let { instance: hakuInstance, module: hakuModule } = await WebAssembly.instantiateStreaming(
    fetch(import.meta.resolve("./wasm/haku.wasm")),
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

/* ------ */

let renderCanvas = document.getElementById("render");
let codeTextArea = document.getElementById("code");
let outputP = document.getElementById("output");

let ctx = renderCanvas.getContext("2d");

function rerender() {
    console.log("rerender");

    let width = renderCanvas.width;
    let height = renderCanvas.height;

    let logs = [];

    let pInstance = w.haku_instance_new();
    let pBrush = w.haku_brush_new();
    let pBitmap = w.haku_bitmap_new(width, height);
    let code = allocString(codeTextArea.value);
    let deallocEverything = () => {
        freeString(code);
        w.haku_bitmap_destroy(pBitmap);
        w.haku_brush_destroy(pBrush);
        w.haku_instance_destroy(pInstance);
        outputP.textContent = logs.join("\n");
    };

    let compileStatusCode = w.haku_compile_brush(pInstance, pBrush, code.length, code.ptr);
    let pCompileStatusString = w.haku_status_string(compileStatusCode);
    logs.push(`compile: ${readCString(pCompileStatusString)}`);

    for (let i = 0; i < w.haku_num_diagnostics(pBrush); ++i) {
        let start = w.haku_diagnostic_start(pBrush, i);
        let end = w.haku_diagnostic_end(pBrush, i);
        let length = w.haku_diagnostic_message_len(pBrush, i);
        let pMessage = w.haku_diagnostic_message(pBrush, i);
        let message = readString(length, pMessage);
        logs.push(`${start}..${end}: ${message}`);
    }

    if (w.haku_num_diagnostics(pBrush) > 0) {
        deallocEverything();
        return;
    }

    let renderStatusCode = w.haku_render_brush(pInstance, pBrush, pBitmap);
    let pRenderStatusString = w.haku_status_string(renderStatusCode);
    logs.push(`render: ${readCString(pRenderStatusString)}`);

    if (w.haku_has_exception(pInstance)) {
        let length = w.haku_exception_message_len(pInstance);
        let pMessage = w.haku_exception_message(pInstance);
        let message = readString(length, pMessage);
        logs.push(`exception: ${message}`);

        deallocEverything();
        return;
    }

    let pBitmapData = w.haku_bitmap_data(pBitmap);
    let bitmapDataBuffer = new Float32Array(memory.buffer, pBitmapData, width * height * 4);
    let imageData = new ImageData(width, height);
    for (let i = 0; i < bitmapDataBuffer.length; ++i) {
        imageData.data[i] = bitmapDataBuffer[i] * 255;
    }
    ctx.putImageData(imageData, 0, 0);

    deallocEverything();
}

rerender();
codeTextArea.addEventListener("input", rerender);
