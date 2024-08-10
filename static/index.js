import { Painter } from "./painter.js";
import { Wall } from "./wall.js";
import { Haku } from "./haku.js";
import { getUserId, newSession, waitForLogin } from "./session.js";
import { debounce } from "./framework.js";

let main = document.querySelector("main");
let canvasRenderer = main.querySelector("rkgk-canvas-renderer");
let reticleRenderer = main.querySelector("rkgk-reticle-renderer");
let brushEditor = main.querySelector("rkgk-brush-editor");

let haku = new Haku();
let painter = new Painter(512);

reticleRenderer.connectViewport(canvasRenderer.viewport);
canvasRenderer.addEventListener(".viewportUpdate", () => reticleRenderer.updateTransform());

// In the background, connect to the server.
(async () => {
    await waitForLogin();
    console.info("login ready! starting session");

    let session = await newSession(getUserId(), localStorage.getItem("rkgk.mostRecentWallId"));
    localStorage.setItem("rkgk.mostRecentWallId", session.wallId);

    let wall = new Wall(session.wallInfo.chunkSize);
    canvasRenderer.initialize(wall);

    for (let onlineUser of session.wallInfo.online) {
        wall.onlineUsers.addUser(onlineUser.sessionId, { nickname: onlineUser.nickname });
    }

    session.addEventListener("error", (event) => console.error(event));
    session.addEventListener("action", (event) => {
        if (event.kind.event == "cursor") {
            let reticle = reticleRenderer.getOrAddReticle(wall.onlineUsers, event.sessionId);
            let { x, y } = event.kind.position;
            reticle.setCursor(x, y);
        }
    });

    let compileBrush = () => haku.setBrush(brushEditor.code);
    compileBrush();
    brushEditor.addEventListener(".codeChanged", () => compileBrush());

    let reportCursor = debounce(1000 / 60, (x, y) => session.reportCursor(x, y));
    canvasRenderer.addEventListener(".cursor", async (event) => {
        reportCursor(event.x, event.y);
    });

    canvasRenderer.addEventListener(".paint", async (event) => {
        painter.renderBrush(haku);
        let imageBitmap = await painter.createImageBitmap();

        let left = event.x - painter.paintArea / 2;
        let top = event.y - painter.paintArea / 2;

        let leftChunk = Math.floor(left / wall.chunkSize);
        let topChunk = Math.floor(top / wall.chunkSize);
        let rightChunk = Math.ceil((left + painter.paintArea) / wall.chunkSize);
        let bottomChunk = Math.ceil((top + painter.paintArea) / wall.chunkSize);
        for (let chunkY = topChunk; chunkY < bottomChunk; ++chunkY) {
            for (let chunkX = leftChunk; chunkX < rightChunk; ++chunkX) {
                let chunk = wall.getOrCreateChunk(chunkX, chunkY);
                let x = Math.floor(-chunkX * wall.chunkSize + left);
                let y = Math.floor(-chunkY * wall.chunkSize + top);
                chunk.ctx.drawImage(imageBitmap, x, y);
            }
        }
        imageBitmap.close();
    });

    session.eventLoop();
})();
