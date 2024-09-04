import { Pixmap } from "rkgk/haku.js";
import { OnlineUsers } from "rkgk/online-users.js";

export class Chunk {
    constructor(size) {
        this.pixmap = new Pixmap(size, size);
        this.canvas = new OffscreenCanvas(size, size);
        this.ctx = this.canvas.getContext("2d");
        this.renderDirty = false;
    }

    syncFromPixmap() {
        this.ctx.putImageData(this.pixmap.getImageData(), 0, 0);
    }

    syncToPixmap() {
        let imageData = this.ctx.getImageData(0, 0, this.canvas.width, this.canvas.height);
        this.pixmap.getImageData().data.set(imageData.data, 0);
    }

    markModified() {
        this.renderDirty = true;
    }
}

export class Wall {
    #chunks = new Map();

    constructor(wallInfo) {
        this.chunkSize = wallInfo.chunkSize;
        this.onlineUsers = new OnlineUsers(wallInfo);
    }

    static chunkKey(x, y) {
        return `(${x},${y})`;
    }

    getChunk(x, y) {
        return this.#chunks.get(Wall.chunkKey(x, y));
    }

    getOrCreateChunk(x, y) {
        let key = Wall.chunkKey(x, y);
        if (this.#chunks.has(key)) {
            return this.#chunks.get(key);
        } else {
            let chunk = new Chunk(this.chunkSize);
            this.#chunks.set(key, chunk);
            return chunk;
        }
    }
}
