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

    #sendJson(object) {
        console.debug("sendJson", object);
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

    async join(wallId) {
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
            await this.joinInner();
        } catch (error) {
            this.#dispatchError(error, "connection", `communication failed: ${error.toString()}`);
        }
    }

    async joinInner() {
        let version = await this.#recvJson();
        console.info("protocol version", version.version);
        // TODO: This should probably verify that the version is compatible.
        // We don't have a way of sending Rust stuff to JavaScript just yet, so we don't care about it.

        if (this.wallId == null) {
            this.#sendJson({ login: "new", user: this.userId });
        } else {
            this.#sendJson({ login: "join", user: this.userId, wall: this.wallId });
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
                    await this.#processEvent(JSON.parse(event.data));
                } else {
                    console.warn("binary event not yet supported");
                }
            }
        } catch (error) {
            this.#dispatchError(error, "protocol", `error in event loop: ${error.toString()}`);
        }
    }

    async #processEvent(event) {
        if (event.kind != null) {
            this.dispatchEvent(
                Object.assign(new Event("action"), {
                    sessionId: event.sessionId,
                    kind: event.kind,
                }),
            );
        }
    }

    async reportCursor(x, y) {
        this.#sendJson({
            event: "cursor",
            position: { x, y },
        });
    }
}

export async function newSession(userId, wallId) {
    let session = new Session(userId);
    await session.join(wallId);
    return session;
}
