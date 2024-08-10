import { Pixmap } from "./haku.js";

export class Painter {
    #pixmap;
    imageBitmap;

    constructor(paintArea) {
        this.paintArea = paintArea;
        this.#pixmap = new Pixmap(paintArea, paintArea);
    }

    async createImageBitmap() {
        return await createImageBitmap(this.#pixmap.imageData);
    }

    renderBrush(haku) {
        this.#pixmap.clear(0, 0, 0, 0);
        let result = haku.renderBrush(this.#pixmap, this.paintArea / 2, this.paintArea / 2);

        return result;
    }
}
