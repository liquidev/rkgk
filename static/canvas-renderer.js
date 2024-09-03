import { listen } from "./framework.js";
import { Viewport } from "./viewport.js";
import { Wall } from "./wall.js";

class CanvasRenderer extends HTMLElement {
    viewport = new Viewport();

    constructor() {
        super();
    }

    connectedCallback() {
        this.canvas = this.appendChild(document.createElement("canvas"));
        this.gl = this.canvas.getContext("webgl2");

        let resizeObserver = new ResizeObserver(() => this.#updateSize());
        resizeObserver.observe(this);

        this.#cursorReportingBehaviour();
        this.#panningBehaviour();
        this.#zoomingBehaviour();
        this.#paintingBehaviour();

        this.addEventListener("contextmenu", (event) => event.preventDefault());
    }

    initialize(wall, painter) {
        this.wall = wall;
        this.painter = painter;
        this.#initializeRenderer();
        requestAnimationFrame(() => this.#render());
    }

    // Rendering

    #updateSize() {
        this.canvas.width = this.clientWidth;
        this.canvas.height = this.clientHeight;
        // Rerender immediately after the canvas is resized, as its contents have now been invalidated.
        this.#render();
    }

    getWindowSize() {
        return {
            width: this.clientWidth,
            height: this.clientHeight,
        };
    }

    getVisibleRect() {
        return this.viewport.getVisibleRect(this.getWindowSize());
    }

    getVisibleChunkRect() {
        let visibleRect = this.viewport.getVisibleRect(this.getWindowSize());
        let left = Math.floor(visibleRect.x / this.wall.chunkSize);
        let top = Math.floor(visibleRect.y / this.wall.chunkSize);
        let right = Math.ceil((visibleRect.x + visibleRect.width) / this.wall.chunkSize);
        let bottom = Math.ceil((visibleRect.y + visibleRect.height) / this.wall.chunkSize);
        return { left, top, right, bottom };
    }

    // Renderer initialization

    #initializeRenderer() {
        console.groupCollapsed("initializeRenderer");

        this.gl.enable(this.gl.BLEND);
        this.gl.blendFunc(this.gl.SRC_ALPHA, this.gl.ONE_MINUS_SRC_ALPHA);

        let renderChunksProgramId = this.#compileProgram(
            // Vertex
            `#version 300 es

            precision highp float;

            struct Rect {
                vec4 position;
                vec4 uv;
            };

            layout (std140) uniform ub_rects { Rect u_rects[512]; };

            uniform vec2 u_screenSize;
            uniform vec2 u_translation;
            uniform vec2 u_scale;

            layout (location = 0) in vec2 a_position;
            out vec2 vf_uv;

            void main() {
                mat4 matProjection = mat4(
                    2.0 / u_screenSize.x, 0.0,                   0.0, 0.0,
                    0.0,                  2.0 / -u_screenSize.y, 0.0, 0.0,
                    0.0,                  0.0,                   1.0, 0.0,
                    -1.0,                 1.0,                   0.0, 1.0
                );
                mat4 matModel = mat4(
                    u_scale.x,       0.0,             0.0, 0.0,
                    0.0,             u_scale.y,       0.0, 0.0,
                    0.0,             0.0,             1.0, 0.0,
                    u_translation.x, u_translation.y, 0.0, 1.0
                );

                Rect rect = u_rects[gl_InstanceID];
                vec2 localPosition = rect.position.xy + a_position * rect.position.zw;
                vec4 screenPosition = floor(matModel * vec4(localPosition, 0.0, 1.0));
                vec4 scenePosition = matProjection * screenPosition;

                vec2 uv = rect.uv.xy + a_position * rect.uv.zw;

                gl_Position = scenePosition;
                vf_uv = uv;
            }
            `,

            // Fragment
            `#version 300 es

            precision highp float;

            uniform sampler2D u_texture;

            in vec2 vf_uv;
            out vec4 f_color;

            void main() {
                f_color = texture(u_texture, vf_uv);
            }
            `,
        );

        this.renderChunksProgram = {
            id: renderChunksProgramId,

            u_screenSize: this.gl.getUniformLocation(renderChunksProgramId, "u_screenSize"),
            u_translation: this.gl.getUniformLocation(renderChunksProgramId, "u_translation"),
            u_scale: this.gl.getUniformLocation(renderChunksProgramId, "u_scale"),
            u_texture: this.gl.getUniformLocation(renderChunksProgramId, "u_texture"),
            ub_rects: this.gl.getUniformBlockIndex(renderChunksProgramId, "ub_rects"),
        };

        console.debug("renderChunksProgram", this.renderChunksProgram);
        console.debug(
            "uniform buffer data size",
            this.gl.getActiveUniformBlockParameter(
                this.renderChunksProgram.id,
                this.renderChunksProgram.ub_rects,
                this.gl.UNIFORM_BLOCK_DATA_SIZE,
            ),
        );

        this.vaoRectMesh = this.gl.createVertexArray();
        this.vboRectMesh = this.gl.createBuffer();

        this.gl.bindVertexArray(this.vaoRectMesh);
        this.gl.bindBuffer(this.gl.ARRAY_BUFFER, this.vboRectMesh);

        let rectMesh = new Float32Array([0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 0]);
        this.gl.bufferData(this.gl.ARRAY_BUFFER, rectMesh, this.gl.STATIC_DRAW);

        this.gl.vertexAttribPointer(0, 2, this.gl.FLOAT, false, 2 * 4, 0);
        this.gl.enableVertexAttribArray(0);

        this.uboRectsData = new Float32Array(new ArrayBuffer(16384));
        this.uboRectsNum = 0;

        this.uboRects = this.gl.createBuffer();
        this.gl.bindBuffer(this.gl.UNIFORM_BUFFER, this.uboRects);
        this.gl.bufferData(this.gl.UNIFORM_BUFFER, this.uboRectsData, this.gl.DYNAMIC_DRAW);

        this.gl.uniformBlockBinding(
            this.renderChunksProgram.id,
            this.renderChunksProgram.ub_rects,
            0,
        );
        this.gl.bindBufferBase(this.gl.UNIFORM_BUFFER, 0, this.uboRects);

        console.debug("initialized buffers", {
            vaoRectMesh: this.vaoRectMesh,
            vboRectMesh: this.vboRectMesh,
            uboRects: this.uboRects,
        });

        this.atlasAllocator = new AtlasAllocator(this.wall.chunkSize, 8);
        this.chunkAllocations = new Map();

        console.debug("initialized atlas allocator", this.atlasAllocator);

        this.chunksThisFrame = new Map();

        console.debug("GL error state", this.gl.getError());

        console.groupEnd();
    }

    #compileShader(kind, source) {
        let shader = this.gl.createShader(kind);

        this.gl.shaderSource(shader, source);
        this.gl.compileShader(shader);

        if (!this.gl.getShaderParameter(shader, this.gl.COMPILE_STATUS)) {
            let error = new Error(`failed to compile shader: ${this.gl.getShaderInfoLog(shader)}`);
            this.gl.deleteShader(shader);
            throw error;
        } else {
            return shader;
        }
    }

    #compileProgram(vertexSource, fragmentSource) {
        let vertexShader = this.#compileShader(this.gl.VERTEX_SHADER, vertexSource);
        let fragmentShader = this.#compileShader(this.gl.FRAGMENT_SHADER, fragmentSource);

        let program = this.gl.createProgram();
        this.gl.attachShader(program, vertexShader);
        this.gl.attachShader(program, fragmentShader);
        this.gl.linkProgram(program);

        this.gl.deleteShader(vertexShader);
        this.gl.deleteShader(fragmentShader);

        if (!this.gl.getProgramParameter(program, this.gl.LINK_STATUS)) {
            let error = new Error(`failed to link program: ${this.gl.getProgramInfoLog(program)}`);
            this.gl.deleteProgram(program);
            throw error;
        } else {
            return program;
        }
    }

    // Renderer

    #render() {
        // NOTE: We should probably render on-demand only when it's needed.
        requestAnimationFrame(() => this.#render());
        this.#renderWall();
    }

    #renderWall() {
        if (this.wall == null) {
            console.debug("wall is not available, skipping rendering");
            return;
        }

        this.gl.viewport(0, 0, this.canvas.width, this.canvas.height);

        this.gl.clearColor(1, 1, 1, 1);
        this.gl.clear(this.gl.COLOR_BUFFER_BIT);

        this.gl.useProgram(this.renderChunksProgram.id);

        this.gl.uniform2f(
            this.renderChunksProgram.u_screenSize,
            this.canvas.width,
            this.canvas.height,
        );

        this.gl.uniform2f(
            this.renderChunksProgram.u_translation,
            this.canvas.width / 2 - this.viewport.panX * this.viewport.zoom,
            this.canvas.height / 2 - this.viewport.panY * this.viewport.zoom,
        );
        this.gl.uniform2f(this.renderChunksProgram.u_scale, this.viewport.zoom, this.viewport.zoom);

        this.#collectChunksThisFrame();

        for (let [i, chunks] of this.chunksThisFrame) {
            let atlas = this.atlasAllocator.atlases[i];
            this.gl.bindTexture(this.gl.TEXTURE_2D, atlas.id);

            this.#resetRectBuffer();
            for (let chunk of chunks) {
                let { i, allocation } = this.getChunkAllocation(chunk.x, chunk.y);
                let atlas = this.atlasAllocator.atlases[i];
                this.#pushRect(
                    chunk.x * this.wall.chunkSize,
                    chunk.y * this.wall.chunkSize,
                    this.wall.chunkSize,
                    this.wall.chunkSize,
                    (allocation.x * atlas.chunkSize) / atlas.textureSize,
                    (allocation.y * atlas.chunkSize) / atlas.textureSize,
                    atlas.chunkSize / atlas.textureSize,
                    atlas.chunkSize / atlas.textureSize,
                );
            }
            this.#drawRects();
        }
    }

    getChunkAllocation(chunkX, chunkY) {
        let key = Wall.chunkKey(chunkX, chunkY);
        if (this.chunkAllocations.has(key)) {
            return this.chunkAllocations.get(key);
        } else {
            let allocation = this.atlasAllocator.alloc(this.gl);
            this.chunkAllocations.set(key, allocation);
            return allocation;
        }
    }

    #collectChunksThisFrame() {
        // NOTE: Not optimal that we don't preserve the arrays anyhow; it would be better if we
        // preserved the allocations.
        this.chunksThisFrame.clear();

        let visibleRect = this.viewport.getVisibleRect(this.getWindowSize());
        let left = Math.floor(visibleRect.x / this.wall.chunkSize);
        let top = Math.floor(visibleRect.y / this.wall.chunkSize);
        let right = Math.ceil((visibleRect.x + visibleRect.width) / this.wall.chunkSize);
        let bottom = Math.ceil((visibleRect.y + visibleRect.height) / this.wall.chunkSize);
        for (let chunkY = top; chunkY < bottom; ++chunkY) {
            for (let chunkX = left; chunkX < right; ++chunkX) {
                let chunk = this.wall.getChunk(chunkX, chunkY);
                if (chunk != null) {
                    if (chunk.renderDirty) {
                        this.#updateChunkTexture(chunkX, chunkY);
                        chunk.renderDirty = false;
                    }

                    let allocation = this.getChunkAllocation(chunkX, chunkY);

                    let array = this.chunksThisFrame.get(allocation.i);
                    if (array == null) {
                        array = [];
                        this.chunksThisFrame.set(allocation.i, array);
                    }

                    array.push({ x: chunkX, y: chunkY });
                }
            }
        }
    }

    #resetRectBuffer() {
        this.uboRectsNum = 0;
    }

    #pushRect(x, y, width, height, u, v, uWidth, vHeight) {
        let lengthOfRect = 8;

        let i = this.uboRectsNum * lengthOfRect;
        this.uboRectsData[i + 0] = x;
        this.uboRectsData[i + 1] = y;
        this.uboRectsData[i + 2] = width;
        this.uboRectsData[i + 3] = height;
        this.uboRectsData[i + 4] = u;
        this.uboRectsData[i + 5] = v;
        this.uboRectsData[i + 6] = uWidth;
        this.uboRectsData[i + 7] = vHeight;
        this.uboRectsNum += 1;

        if (this.uboRectsNum == ((this.uboRectsData.length / lengthOfRect) | 0)) {
            this.#drawRects();
            this.#resetRectBuffer();
        }
    }

    #drawRects() {
        let rectBuffer = this.uboRectsData.subarray(0, this.uboRectsNum * 8);
        this.gl.bindBuffer(this.gl.UNIFORM_BUFFER, this.uboRects);
        this.gl.bufferSubData(this.gl.UNIFORM_BUFFER, 0, rectBuffer);

        this.gl.bindVertexArray(this.vaoRectMesh);
        this.gl.bindBuffer(this.gl.ARRAY_BUFFER, this.vboRectMesh);
        this.gl.drawArraysInstanced(this.gl.TRIANGLES, 0, 6, this.uboRectsNum);
    }

    #updateChunkTexture(chunkX, chunkY) {
        let allocation = this.getChunkAllocation(chunkX, chunkY);
        let chunk = this.wall.getChunk(chunkX, chunkY);
        this.atlasAllocator.upload(this.gl, allocation, chunk.pixmap);
    }

    // Behaviours

    async #cursorReportingBehaviour() {
        while (true) {
            let event = await listen([this, "mousemove"]);
            let [x, y] = this.viewport.toViewportSpace(
                event.clientX - this.clientLeft,
                event.offsetY - this.clientTop,
                this.getWindowSize(),
            );
            this.dispatchEvent(Object.assign(new Event(".cursor"), { x, y }));
        }
    }

    sendViewportUpdate() {
        this.dispatchEvent(new Event(".viewportUpdate"));
    }

    async #panningBehaviour() {
        while (true) {
            let mouseDown = await listen([this, "mousedown"]);
            if (mouseDown.button == 1 || mouseDown.button == 2) {
                mouseDown.preventDefault();
                while (true) {
                    let event = await listen([window, "mousemove"], [window, "mouseup"]);
                    if (event.type == "mousemove") {
                        this.viewport.panAround(event.movementX, event.movementY);
                        this.sendViewportUpdate();
                    } else if (event.type == "mouseup" && event.button == mouseDown.button) {
                        this.dispatchEvent(new Event(".viewportUpdateEnd"));
                        break;
                    }
                }
            }
        }
    }

    async #zoomingBehaviour() {
        while (true) {
            let event = await listen([this, "wheel"]);

            // TODO: Touchpad zoom
            this.viewport.zoomIn(event.deltaY > 0 ? -1 : 1);
            this.sendViewportUpdate();
            this.dispatchEvent(new Event(".viewportUpdateEnd"));
        }
    }

    async #paintingBehaviour() {
        const paint = (x, y) => {
            let [wallX, wallY] = this.viewport.toViewportSpace(x, y, this.getWindowSize());
            this.dispatchEvent(Object.assign(new Event(".paint"), { x: wallX, y: wallY }));
        };

        while (true) {
            let mouseDown = await listen([this, "mousedown"]);
            if (mouseDown.button == 0) {
                paint(mouseDown.offsetX, mouseDown.offsetY);
                while (true) {
                    let event = await listen([window, "mousemove"], [window, "mouseup"]);
                    if (event.type == "mousemove") {
                        paint(event.clientX - this.clientLeft, event.offsetY - this.clientTop);
                    } else if (event.type == "mouseup") {
                        break;
                    }
                }
            }
        }
    }
}

customElements.define("rkgk-canvas-renderer", CanvasRenderer);

class Atlas {
    static getInitBuffer(chunkSize, nChunks) {
        let imageSize = chunkSize * nChunks;
        return new Uint8Array(imageSize * imageSize * 4);
    }

    constructor(gl, chunkSize, nChunks, initBuffer) {
        this.id = gl.createTexture();
        this.chunkSize = chunkSize;
        this.nChunks = nChunks;
        this.textureSize = chunkSize * nChunks;

        this.free = Array(nChunks * nChunks);
        for (let y = 0; y < nChunks; ++y) {
            for (let x = 0; x < nChunks; ++x) {
                this.free[x + y * nChunks] = { x, y };
            }
        }

        gl.bindTexture(gl.TEXTURE_2D, this.id);
        gl.texImage2D(
            gl.TEXTURE_2D,
            0,
            gl.RGBA8,
            this.textureSize,
            this.textureSize,
            0,
            gl.RGBA,
            gl.UNSIGNED_BYTE,
            initBuffer,
        );
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    }

    alloc() {
        return this.free.pop();
    }

    upload(gl, { x, y }, pixmap) {
        gl.bindTexture(gl.TEXTURE_2D, this.id);
        gl.texSubImage2D(
            gl.TEXTURE_2D,
            0,
            x * this.chunkSize,
            y * this.chunkSize,
            this.chunkSize,
            this.chunkSize,
            gl.RGBA,
            gl.UNSIGNED_BYTE,
            pixmap.getArrayBuffer(),
        );
    }
}

class AtlasAllocator {
    atlases = [];

    constructor(chunkSize, nChunks) {
        this.chunkSize = chunkSize;
        this.nChunks = nChunks;
        this.initBuffer = Atlas.getInitBuffer(chunkSize, nChunks);
    }

    alloc(gl) {
        // Right now we do a dumb linear scan through all atlases, but in the future it would be
        // really nice to optimize this by storing information about which atlases have free slots
        // precisely.

        for (let i = 0; i < this.atlases.length; ++i) {
            let atlas = this.atlases[i];
            let allocation = atlas.alloc();
            if (allocation != null) {
                return { i, allocation };
            }
        }

        let i = this.atlases.length;
        let atlas = new Atlas(gl, this.chunkSize, this.nChunks, this.initBuffer);
        let allocation = atlas.alloc();
        this.atlases.push(atlas);
        return { i, allocation };
    }

    upload(gl, { i, allocation }, pixmap) {
        this.atlases[i].upload(gl, allocation, pixmap);
    }
}
