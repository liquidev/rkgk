export class Reticle extends HTMLElement {
    #kind = null;
    #data = {};

    #container;

    constructor(nickname) {
        super();
        this.nickname = nickname;
    }

    connectedCallback() {
        this.style.setProperty("--color", this.getColor());

        this.#container = this.appendChild(document.createElement("div"));
        this.#container.classList.add("container");
    }

    getColor() {
        let hash = 5381;
        for (let i = 0; i < this.nickname.length; ++i) {
            hash <<= 5;
            hash += hash;
            hash += this.nickname.charCodeAt(i);
            hash &= 0xffff;
        }
        return `oklch(70% 0.2 ${(hash / 0xffff) * 360}deg)`;
    }

    #update(kind, data) {
        this.#data = data;

        if (kind != this.#kind) {
            this.classList = "";
            this.#container.replaceChildren();
            this.#kind = kind;
        }

        this.dispatchEvent(new Event(".update"));
    }

    setCursor(x, y) {
        this.#update("cursor", { x, y });
    }

    render(viewport, windowSize) {
        if (!this.rendered) {
            if (this.#kind == "cursor") {
                this.classList.add("cursor");

                let arrow = this.#container.appendChild(document.createElement("div"));
                arrow.classList.add("arrow");

                let nickname = this.#container.appendChild(document.createElement("div"));
                nickname.classList.add("nickname");
                nickname.textContent = this.nickname;
            }
        }

        if (this.#kind == "cursor") {
            let { x, y } = this.#data;
            let [viewportX, viewportY] = viewport.toScreenSpace(x, y, windowSize);
            this.style.transform = `translate(${viewportX}px, ${viewportY}px)`;
        }

        this.rendered = true;
    }
}

customElements.define("rkgk-reticle", Reticle);

export class ReticleRenderer extends HTMLElement {
    #reticles = new Map();
    #reticlesDiv;

    connectedCallback() {
        this.#reticlesDiv = this.appendChild(document.createElement("div"));
        this.#reticlesDiv.classList.add("reticles");

        this.updateTransform();
        let resizeObserver = new ResizeObserver(() => this.updateTransform());
        resizeObserver.observe(this);
    }

    connectViewport(viewport) {
        this.viewport = viewport;
        this.updateTransform();
    }

    getOrAddReticle(onlineUsers, sessionId) {
        if (this.#reticles.has(sessionId)) {
            return this.#reticles.get(sessionId);
        } else {
            let reticle = new Reticle(onlineUsers.getUser(sessionId).nickname);
            reticle.addEventListener(".update", () => {
                if (this.viewport != null) {
                    reticle.render(this.viewport, {
                        width: this.clientWidth,
                        height: this.clientHeight,
                    });
                }
            });
            this.#reticles.set(sessionId, reticle);
            this.#reticlesDiv.appendChild(reticle);
            return reticle;
        }
    }

    removeReticle(sessionId) {
        if (this.#reticles.has(sessionId)) {
            let reticle = this.#reticles.get(sessionId);
            this.#reticles.delete(sessionId);
            this.#reticlesDiv.removeChild(reticle);
        }
    }

    updateTransform() {
        if (this.viewport == null) {
            console.debug("viewport is disconnected, skipping transform update");
            return;
        }

        let windowSize = { width: this.clientWidth, height: this.clientHeight };
        for (let [_, reticle] of this.#reticles) {
            reticle.render(this.viewport, windowSize);
        }
    }
}

customElements.define("rkgk-reticle-renderer", ReticleRenderer);
