export class Painter {
    constructor(paintArea) {
        this.paintArea = paintArea;
    }

    renderBrushToWall(haku, centerX, centerY, wall) {
        haku.resetVm();

        let evalResult = haku.evalBrush();
        if (evalResult.status != "ok")
            return { status: "error", phase: "eval", result: evalResult };

        let left = centerX - this.paintArea / 2;
        let top = centerY - this.paintArea / 2;

        let leftChunk = Math.floor(left / wall.chunkSize);
        let topChunk = Math.floor(top / wall.chunkSize);
        let rightChunk = Math.ceil((left + this.paintArea) / wall.chunkSize);
        let bottomChunk = Math.ceil((top + this.paintArea) / wall.chunkSize);
        for (let chunkY = topChunk; chunkY < bottomChunk; ++chunkY) {
            for (let chunkX = leftChunk; chunkX < rightChunk; ++chunkX) {
                let x = Math.floor(-chunkX * wall.chunkSize + centerX);
                let y = Math.floor(-chunkY * wall.chunkSize + centerY);
                let chunk = wall.getOrCreateChunk(chunkX, chunkY);
                chunk.markModified();

                let renderResult = haku.renderValue(chunk.pixmap, x, y);
                if (renderResult.status != "ok") {
                    return { status: "error", phase: "render", result: renderResult };
                }
            }
        }

        for (let y = topChunk; y < bottomChunk; ++y) {
            for (let x = leftChunk; x < rightChunk; ++x) {
                let chunk = wall.getChunk(x, y);
                chunk.syncFromPixmap();
            }
        }

        return { status: "ok" };
    }
}
