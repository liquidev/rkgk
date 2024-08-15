import { listen } from "./framework.js";

let loginStorage = JSON.parse(localStorage.getItem("rkgk.login") ?? "{}");

function saveLoginStorage() {
    localStorage.setItem("rkgk.login", JSON.stringify(loginStorage));
}

let resolveLoggedInPromise;
let loggedInPromise = new Promise((resolve) => (resolveLoggedInPromise = resolve));

export function isUserLoggedIn() {
    return loginStorage.userId != null;
}

export function getUserId() {
    return loginStorage.userId;
}

export function waitForLogin() {
    return loggedInPromise;
}

if (isUserLoggedIn()) {
    resolveLoggedInPromise();
}

export async function registerUser(nickname) {
    try {
        let response = await fetch("/api/login", {
            method: "POST",
            body: JSON.stringify({ nickname }),
            headers: {
                "Content-Type": "application/json",
            },
        });

        if (response.status == 500) {
            console.error("login service returned 500 status", response);
            return {
                status: "error",
                message:
                    "We're sorry, but we ran into some trouble registering your account. Please try again.",
            };
        }

        let responseText = await response.text();
        let responseJson = JSON.parse(responseText);
        if (responseJson.status != "ok") {
            console.error("registering user failed", responseJson);
            return {
                status: "error",
                message: "Something seems to have gone wrong. Please try again.",
            };
        }

        console.log(responseJson);
        loginStorage.userId = responseJson.userId;
        console.info("user registered", loginStorage.userId);
        saveLoginStorage();
        resolveLoggedInPromise();

        return { status: "ok" };
    } catch (error) {
        console.error("registering user failed", error);
        return {
            status: "error",
            message: "Something seems to have gone wrong. Please try again.",
        };
    }
}

class Session extends EventTarget {
    constructor(userId) {
        super();
        this.userId = userId;
    }

    async #recvJson() {
        let event = await listen([this.ws, "message"]);
        if (typeof event.data == "string") {
            return JSON.parse(event.data);
        } else {
            throw new Error("received a binary message where a JSON text message was expected");
        }
    }

    async #recvBinary() {
        let event = await listen([this.ws, "message"]);
        if (event.data instanceof Blob) {
            return event.data;
        } else {
            throw new Error("received a text message where a binary message was expected");
        }
    }

    #sendJson(object) {
        this.ws.send(JSON.stringify(object));
    }

    #dispatchError(source, kind, message) {
        this.dispatchEvent(
            Object.assign(new Event("error"), {
                source,
                errorKind: kind,
                message,
            }),
        );
    }

    async join(wallId, userInit) {
        console.info("joining wall", wallId);
        this.wallId = wallId;

        this.ws = new WebSocket("/api/wall");

        this.ws.addEventListener("error", (event) => {
            console.error("WebSocket connection error", error);
            this.dispatchEvent(Object.assign(new Event("error"), event));
        });

        this.ws.addEventListener("message", (event) => {
            if (typeof event.data == "string") {
                let json = JSON.parse(event.data);
                if (json.error != null) {
                    console.error("received error from server:", json.error);
                    this.#dispatchError(json, "protocol", json.error);
                }
            }
        });

        try {
            await listen([this.ws, "open"]);
            await this.joinInner(wallId, userInit);
        } catch (error) {
            this.#dispatchError(error, "connection", `communication failed: ${error.toString()}`);
        }
    }

    async joinInner(wallId, userInit) {
        let version = await this.#recvJson();
        console.info("protocol version", version.version);
        // TODO: This should probably verify that the version is compatible.
        // We don't have a way of sending Rust stuff to JavaScript just yet, so we don't care about it.

        let init = {
            brush: userInit.brush,
        };
        if (this.wallId == null) {
            this.#sendJson({
                user: this.userId,
                init,
            });
        } else {
            this.#sendJson({
                user: this.userId,
                wall: wallId,
                init,
            });
        }

        let loginResponse = await this.#recvJson();
        if (loginResponse.response == "loggedIn") {
            this.wallId = loginResponse.wall;
            this.wallInfo = loginResponse.wallInfo;
            this.sessionId = loginResponse.sessionId;

            console.info("logged in", this.wallId, this.sessionId);
            console.info("wall info:", this.wallInfo);
        } else {
            this.#dispatchError(
                loginResponse,
                loginResponse.response,
                "login failed; check error kind for details",
            );
            return;
        }
    }

    async eventLoop() {
        try {
            while (true) {
                let event = await listen([this.ws, "message"]);
                if (typeof event.data == "string") {
                    await this.#processNotify(JSON.parse(event.data));
                } else {
                    console.warn("unhandled binary event", event.data);
                }
            }
        } catch (error) {
            this.#dispatchError(error, "protocol", `error in event loop: ${error.toString()}`);
        }
    }

    async #processNotify(notify) {
        if (notify.notify == "wall") {
            this.dispatchEvent(
                Object.assign(new Event("wallEvent"), {
                    sessionId: notify.sessionId,
                    wallEvent: notify.wallEvent,
                }),
            );
        }

        if (notify.notify == "chunks") {
            let chunkData = await this.#recvBinary();
            this.dispatchEvent(
                Object.assign(new Event("chunks"), {
                    chunkInfo: notify.chunks,
                    chunkData,
                }),
            );
        }
    }

    sendCursor(x, y) {
        this.#sendJson({
            request: "wall",
            wallEvent: {
                event: "cursor",
                position: { x, y },
            },
        });
    }

    sendPlot(points) {
        this.#sendJson({
            request: "wall",
            wallEvent: {
                event: "plot",
                points,
            },
        });
    }

    sendSetBrush(brush) {
        this.#sendJson({
            request: "wall",
            wallEvent: {
                event: "setBrush",
                brush,
            },
        });
    }

    sendViewport({ left, top, right, bottom }) {
        this.#sendJson({
            request: "viewport",
            topLeft: { x: left, y: top },
            bottomRight: { x: right, y: bottom },
        });
    }

    sendMoreChunks() {
        this.#sendJson({
            request: "moreChunks",
        });
    }
}

export async function newSession(userId, wallId, userInit) {
    let session = new Session(userId);
    await session.join(wallId, userInit);
    return session;
}
