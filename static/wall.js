import { OnlineUsers } from "./online-users.js";

export class Chunk {
    constructor(size) {
        this.canvas = new OffscreenCanvas(size, size);
        this.ctx = this.canvas.getContext("2d");
    }
}

export class Wall {
    #chunks = new Map();
    onlineUsers = new OnlineUsers();

    constructor(chunkSize) {
        this.chunkSize = chunkSize;
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
