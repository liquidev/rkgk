import { Wall } from "rkgk/wall.js";
import {
    getLoginSecret,
    getUserId,
    isUserLoggedIn,
    newSession,
    registerUser,
    waitForLogin,
} from "rkgk/session.js";
import { debounce } from "rkgk/framework.js";
import { ReticleCursor } from "rkgk/reticle-renderer.js";

const updateInterval = 1000 / 60;

let main = document.querySelector("main");
let canvasRenderer = main.querySelector("rkgk-canvas-renderer");
let reticleRenderer = main.querySelector("rkgk-reticle-renderer");
let brushEditor = main.querySelector("rkgk-brush-editor");
let brushPreview = main.querySelector("rkgk-brush-preview");
let welcome = main.querySelector("rkgk-welcome");
let connectionStatus = main.querySelector("rkgk-connection-status");

document.getElementById("js-loading").remove();
reticleRenderer.connectViewport(canvasRenderer.viewport);

function updateUrl(session, viewport) {
    let url = new URL(window.location);
    url.hash = `${session.wallId}&x=${Math.floor(viewport.panX)}&y=${Math.floor(viewport.panY)}&zoom=${viewport.zoomLevel}`;
    history.replaceState(null, "", url);
}

function readUrl(urlString) {
    let url = new URL(urlString);
    let fragments = url.hash.substring(1).split("&");

    let wallId = null;
    let viewport = { x: 0, y: 0, zoom: 0 };

    if (fragments.length == 0) return { wallId, viewport };
    if (fragments[0].startsWith("wall_") && fragments[0].length == 48) {
        wallId = fragments[0];
    }

    for (let i = 1; i < fragments.length; ++i) {
        let pair = fragments[i].split("=");
        if (pair.length != 2) continue;

        let [key, value] = pair;
        try {
            if (key == "x") viewport.x = parseFloat(value);
            if (key == "y") viewport.y = parseFloat(value);
            if (key == "zoom") viewport.zoom = parseFloat(value);
        } catch (error) {
            console.error(`broken fragment url value: ${key}=${value}`);
        }
    }

    return { wallId, viewport };
}

// In the background, connect to the server.
(async () => {
    console.info("checking for user registration status");
    if (!isUserLoggedIn()) {
        await welcome.show({
            async onRegister(nickname) {
                return await registerUser(nickname);
            },
        });
    }

    connectionStatus.showLoggingIn();
    await waitForLogin();
    console.info("login ready! starting session");

    let urlData = readUrl(window.location);
    canvasRenderer.viewport.panX = urlData.viewport.x;
    canvasRenderer.viewport.panY = urlData.viewport.y;
    canvasRenderer.viewport.zoomLevel = urlData.viewport.zoom;

    let session = await newSession({
        userId: getUserId(),
        secret: getLoginSecret(),
        wallId: urlData.wallId ?? localStorage.getItem("rkgk.mostRecentWallId"),
        userInit: {
            brush: brushEditor.code,
        },

        onError(error) {
            connectionStatus.showError(error.source);
        },

        async onDisconnect() {
            let duration = 5000 + Math.random() * 1000;
            while (true) {
                console.info("waiting a bit for the server to come back up", duration);
                await connectionStatus.showDisconnected(duration);
                try {
                    console.info("trying to reconnect");
                    let response = await fetch("/auto-reload/back-up");
                    if (response.status == 200) {
                        window.location.reload();
                        break;
                    }
                } catch (e) {}
                duration = duration * 1.618033989 + Math.random() * 1000;
            }
        },
    });
    localStorage.setItem("rkgk.mostRecentWallId", session.wallId);

    connectionStatus.hideLoggingIn();

    updateUrl(session, canvasRenderer.viewport);

    addEventListener("hashchange", (event) => {
        let newUrlData = readUrl(event.newURL);
        if (newUrlData.wallId != urlData.wallId) {
            // Different wall, reload the app.
            window.location.reload();
        } else {
            canvasRenderer.viewport.panX = newUrlData.viewport.x;
            canvasRenderer.viewport.panY = newUrlData.viewport.y;
            canvasRenderer.viewport.zoomLevel = newUrlData.viewport.zoom;
            canvasRenderer.sendViewportUpdate();
        }
    });

    let wall = new Wall(session.wallInfo);
    canvasRenderer.initialize(wall);

    for (let onlineUser of session.wallInfo.online) {
        wall.onlineUsers.addUser(onlineUser.sessionId, {
            nickname: onlineUser.nickname,
            brush: onlineUser.init.brush,
        });
    }

    let currentUser = wall.onlineUsers.getUser(session.sessionId);

    session.addEventListener("error", (event) => console.error(event));

    session.addEventListener("wallEvent", (event) => {
        let wallEvent = event.wallEvent;
        if (wallEvent.sessionId != session.sessionId) {
            if (wallEvent.kind.event == "join") {
                wall.onlineUsers.addUser(wallEvent.sessionId, {
                    nickname: wallEvent.kind.nickname,
                    brush: wallEvent.kind.init.brush,
                });
            }

            let user = wall.onlineUsers.getUser(wallEvent.sessionId);
            if (user == null) {
                console.warn("received event for an unknown user", wallEvent);
                return;
            }

            if (wallEvent.kind.event == "leave") {
                if (user.reticle != null) {
                    reticleRenderer.removeReticle(user.reticle);
                }
                wall.onlineUsers.removeUser(wallEvent.sessionId);
            }

            if (wallEvent.kind.event == "cursor") {
                if (user.reticle == null) {
                    user.reticle = new ReticleCursor(
                        wall.onlineUsers.getUser(wallEvent.sessionId).nickname,
                    );
                    reticleRenderer.addReticle(user.reticle);
                }

                let { x, y } = wallEvent.kind.position;
                user.reticle.setCursor(x, y);
            }

            if (wallEvent.kind.event == "setBrush") {
                user.setBrush(wallEvent.kind.brush);
            }

            if (wallEvent.kind.event == "plot") {
                for (let { x, y } of wallEvent.kind.points) {
                    user.renderBrushToChunks(wall, x, y);
                }
            }
        }
    });

    let sendViewportUpdate = debounce(updateInterval, () => {
        let visibleRect = canvasRenderer.getVisibleChunkRect();
        session.sendViewport(visibleRect);
    });
    canvasRenderer.addEventListener(".viewportUpdate", sendViewportUpdate);
    sendViewportUpdate();

    session.addEventListener("chunks", async (event) => {
        let { chunkInfo, chunkData, hasMore } = event;

        console.debug("received data for chunks", {
            chunkInfo,
            chunkDataSize: chunkData.size,
        });

        let updatePromises = [];
        for (let info of event.chunkInfo) {
            if (info.length > 0) {
                let blob = chunkData.slice(info.offset, info.offset + info.length, "image/webp");
                updatePromises.push(
                    createImageBitmap(blob).then((bitmap) => {
                        let chunk = wall.getOrCreateChunk(info.position.x, info.position.y);
                        chunk.ctx.globalCompositeOperation = "copy";
                        chunk.ctx.drawImage(bitmap, 0, 0);
                        chunk.syncToPixmap();
                        chunk.markModified();
                    }),
                );
            }
        }

        await Promise.all(updatePromises);
        if (hasMore) {
            console.info("more chunks are pending; requesting more");
            session.sendMoreChunks();
        }
    });

    let reportCursor = debounce(updateInterval, (x, y) => session.sendCursor(x, y));
    canvasRenderer.addEventListener(".cursor", async (event) => {
        reportCursor(event.x, event.y);
    });

    let plotQueue = [];
    async function flushPlotQueue() {
        let points = plotQueue.splice(0, plotQueue.length);
        if (points.length != 0) {
            session.sendPlot(points);
        }
    }

    setInterval(flushPlotQueue, updateInterval);

    canvasRenderer.addEventListener(".paint", async (event) => {
        plotQueue.push({ x: event.x, y: event.y });

        if (currentUser.isBrushOk) {
            brushEditor.resetErrors();

            let result = currentUser.renderBrushToChunks(wall, event.x, event.y);
            if (result.status == "error") {
                brushEditor.renderHakuResult(
                    result.phase == "eval" ? "Evaluation" : "Rendering",
                    result.result,
                );
            }
        }
    });

    canvasRenderer.addEventListener(".viewportUpdate", () => reticleRenderer.render());
    canvasRenderer.addEventListener(".viewportUpdateEnd", () =>
        updateUrl(session, canvasRenderer.viewport),
    );

    function compileBrush() {
        let compileResult = currentUser.setBrush(brushEditor.code);
        brushEditor.renderHakuResult("Compilation", compileResult);

        if (compileResult.status != "ok") {
            brushPreview.setErrorFlag();
            return;
        }

        let previewResult = brushPreview.renderBrush(currentUser.haku);
        if (previewResult.status == "error") {
            brushEditor.renderHakuResult(
                previewResult.phase == "eval" ? "Evaluation" : "Rendering",
                previewResult.result,
            );
        }
    }

    compileBrush();
    brushEditor.addEventListener(".codeChanged", async () => {
        flushPlotQueue();
        compileBrush();
        session.sendSetBrush(brushEditor.code);
    });

    session.eventLoop();
})();
