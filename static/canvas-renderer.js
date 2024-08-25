import { listen } from "./framework.js";
import { Viewport } from "./viewport.js";

class CanvasRenderer extends HTMLElement {
    viewport = new Viewport();

    constructor() {
        super();
    }

    connectedCallback() {
        this.canvas = this.appendChild(document.createElement("canvas"));
        this.ctx = this.canvas.getContext("2d");

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
        requestAnimationFrame(() => this.#render());
    }

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

    #render() {
        // NOTE: We should probably render on-demand only when it's needed.
        requestAnimationFrame(() => this.#render());

        this.#renderWall();
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

    #renderWall() {
        if (this.wall == null) {
            console.debug("wall is not available, skipping rendering");
            return;
        }

        this.ctx.fillStyle = "white";
        this.ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);

        this.ctx.save();
        this.ctx.translate(Math.floor(this.clientWidth / 2), Math.floor(this.clientHeight / 2));
        this.ctx.scale(this.viewport.zoom, this.viewport.zoom);
        this.ctx.translate(-this.viewport.panX, -this.viewport.panY);

        let visibleRect = this.viewport.getVisibleRect(this.getWindowSize());
        let left = Math.floor(visibleRect.x / this.wall.chunkSize);
        let top = Math.floor(visibleRect.y / this.wall.chunkSize);
        let right = Math.ceil((visibleRect.x + visibleRect.width) / this.wall.chunkSize);
        let bottom = Math.ceil((visibleRect.y + visibleRect.height) / this.wall.chunkSize);
        for (let chunkY = top; chunkY < bottom; ++chunkY) {
            for (let chunkX = left; chunkX < right; ++chunkX) {
                let x = chunkX * this.wall.chunkSize;
                let y = chunkY * this.wall.chunkSize;

                let chunk = this.wall.getChunk(chunkX, chunkY);
                if (chunk != null) {
                    this.ctx.globalCompositeOperation = "source-over";
                    this.ctx.imageSmoothingEnabled = false;
                    this.ctx.drawImage(chunk.canvas, x, y);
                }
            }
        }

        this.ctx.restore();

        if (this.ctx.brushPreview != null) {
            this.ctx.drawImage(this.ctx.brushPreview, 0, 0);
        }
    }

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
