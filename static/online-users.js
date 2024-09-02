import { Haku } from "./haku.js";
import { Painter } from "./painter.js";

export class User {
    nickname = "";
    brush = "";
    reticle = null;

    isBrushOk = false;

    constructor(wallInfo, nickname) {
        this.nickname = nickname;

        this.haku = new Haku(wallInfo.hakuLimits);
        this.painter = new Painter(wallInfo.paintArea);
    }

    destroy() {
        this.haku.destroy();
    }

    setBrush(brush) {
        console.group("setBrush", this.nickname);
        let compileResult = this.haku.setBrush(brush);
        console.log("compiling brush complete", compileResult);
        console.groupEnd();

        this.isBrushOk = compileResult.status == "ok";

        return compileResult;
    }

    renderBrushToChunks(wall, x, y) {
        console.group("renderBrushToChunks", this.nickname);
        let result = this.painter.renderBrushToWall(this.haku, x, y, wall);
        console.log("rendering brush to chunks complete");
        console.groupEnd();

        return result;
    }
}

export class OnlineUsers extends EventTarget {
    #wallInfo;
    #users = new Map();

    constructor(wallInfo) {
        super();

        this.#wallInfo = wallInfo;
    }

    addUser(sessionId, { nickname, brush }) {
        if (!this.#users.has(sessionId)) {
            console.info("user added", sessionId, nickname);

            let user = new User(this.#wallInfo, nickname);
            user.setBrush(brush);
            this.#users.set(sessionId, user);
            return user;
        } else {
            console.info("user already exists", sessionId, nickname);
            return this.#users.get(sessionId);
        }
    }

    getUser(sessionId) {
        return this.#users.get(sessionId);
    }

    removeUser(sessionId) {
        if (this.#users.has(sessionId)) {
            let user = this.#users.get(sessionId);
            user.destroy();
            console.info("user removed", sessionId, user.nickname);
            // TODO: Cleanup reticles
            this.#users.delete(sessionId);
        }
    }
}
