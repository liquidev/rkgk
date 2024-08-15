export class Reticle extends HTMLElement {
    render(_viewport, _windowSize) {
        throw new Error("Reticle.render must be overridden");
    }
}

export class ReticleCursor extends Reticle {
    #container;

    constructor(nickname) {
        super();
        this.nickname = nickname;
    }

    connectedCallback() {
        this.style.setProperty("--color", this.getColor());

        this.#container = this.appendChild(document.createElement("div"));
        this.#container.classList.add("container");

        this.classList.add("cursor");

        let arrow = this.#container.appendChild(document.createElement("div"));
        arrow.classList.add("arrow");

        let nickname = this.#container.appendChild(document.createElement("div"));
        nickname.classList.add("nickname");
        nickname.textContent = this.nickname;
    }

    getColor() {
        let hash = 8803;
        for (let i = 0; i < this.nickname.length; ++i) {
            hash = (hash << 5) - hash + this.nickname.charCodeAt(i);
            hash |= 0;
        }
        return `oklch(65% 0.2 ${(hash / 0xffff) * 360}deg)`;
    }

    setCursor(x, y) {
        this.x = x;
        this.y = y;
        this.dispatchEvent(new Event(".update"));
    }

    render(viewport, windowSize) {
        let [viewportX, viewportY] = viewport.toScreenSpace(this.x, this.y, windowSize);
        this.style.transform = `translate(${viewportX}px, ${viewportY}px)`;
    }
}

customElements.define("rkgk-reticle-cursor", ReticleCursor);

export class ReticleRenderer extends HTMLElement {
    #reticles = new Set();
    #reticlesDiv;

    connectedCallback() {
        this.#reticlesDiv = this.appendChild(document.createElement("div"));
        this.#reticlesDiv.classList.add("reticles");

        this.render();
        let resizeObserver = new ResizeObserver(() => this.render());
        resizeObserver.observe(this);
    }

    connectViewport(viewport) {
        this.viewport = viewport;
        this.render();
    }

    addReticle(reticle) {
        if (!this.#reticles.has(reticle)) {
            reticle.addEventListener(".update", () => {
                if (this.viewport != null) {
                    reticle.render(this.viewport, {
                        width: this.clientWidth,
                        height: this.clientHeight,
                    });
                }
            });
            this.#reticles.add(reticle);
            this.#reticlesDiv.appendChild(reticle);
        }
    }

    removeReticle(reticle) {
        if (this.#reticles.has(reticle)) {
            this.#reticles.delete(reticle);
            this.#reticlesDiv.removeChild(reticle);
        }
    }

    render() {
        if (this.viewport == null) {
            console.debug("viewport is disconnected, skipping transform update");
            return;
        }

        let windowSize = { width: this.clientWidth, height: this.clientHeight };
        for (let reticle of this.#reticles.values()) {
            reticle.render(this.viewport, windowSize);
        }
    }
}

customElements.define("rkgk-reticle-renderer", ReticleRenderer);
