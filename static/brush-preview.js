import { Pixmap } from "rkgk/haku.js";

export class BrushPreview extends HTMLElement {
    constructor() {
        super();
    }

    connectedCallback() {
        this.canvas = this.appendChild(document.createElement("canvas"));
        this.ctx = this.canvas.getContext("2d");
        this.#resizeCanvas();
    }

    #resizeCanvas() {
        this.canvas.width = this.clientWidth;
        this.canvas.height = this.clientHeight;

        if (this.pixmap != null) {
            this.pixmap.destroy();
        }
        this.pixmap = new Pixmap(this.canvas.width, this.canvas.height);
    }

    #renderBrushInner(haku) {
        haku.resetVm();

        let evalResult = haku.evalBrush();
        if (evalResult.status != "ok") {
            return { status: "error", phase: "eval", result: evalResult };
        }

        this.pixmap.clear();
        let renderResult = haku.renderValue(
            this.pixmap,
            this.canvas.width / 2,
            this.canvas.height / 2,
        );
        if (renderResult.status != "ok") {
            return { status: "error", phase: "render", result: renderResult };
        }

        this.ctx.putImageData(this.pixmap.getImageData(), 0, 0);

        return { status: "ok" };
    }

    renderBrush(haku) {
        this.unsetErrorFlag();
        let result = this.#renderBrushInner(haku);
        if (result.status == "error") {
            this.setErrorFlag();
        }
        return result;
    }

    unsetErrorFlag() {
        this.classList.remove("error");
    }

    setErrorFlag() {
        this.classList.add("error");
    }
}

customElements.define("rkgk-brush-preview", BrushPreview);
