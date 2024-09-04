import { listen } from "./framework.js";

export class ResizeHandle extends HTMLElement {
    constructor() {
        super();
    }

    connectedCallback() {
        this.direction = this.getAttribute("data-direction");
        this.targetId = this.getAttribute("data-target");
        this.target = document.getElementById(this.targetId);
        this.targetProperty = this.getAttribute("data-target-property");
        this.initSize = parseInt(this.getAttribute("data-init-size"));
        this.minSize = parseInt(this.getAttribute("data-min-size"));

        this.#setSize(parseInt(localStorage.getItem(this.#localStorageKey)));
        this.#saveSize();
        this.#updateTargetProperty();

        this.visual = this.appendChild(document.createElement("div"));
        this.visual.classList.add("visual");

        this.#draggingBehaviour();
    }

    #setSize(newSize) {
        if (newSize != newSize) {
            newSize = this.initSize;
        }
        this.size = Math.max(this.minSize, newSize);
    }

    get #localStorageKey() {
        return `rkgk.resizeHandle.size.${this.targetId}`;
    }

    #saveSize() {
        localStorage.setItem(this.#localStorageKey, this.size);
    }

    #updateTargetProperty() {
        this.target.style.setProperty(this.targetProperty, `${this.size}px`);
    }

    async #draggingBehaviour() {
        while (true) {
            let mouseDown = await listen([this, "mousedown"]);
            let startingSize = this.size;
            if (mouseDown.button == 0) {
                this.classList.add("dragging");

                while (true) {
                    let event = await listen([window, "mousemove"], [window, "mouseup"]);
                    if (event.type == "mousemove") {
                        if (this.direction == "vertical") {
                            this.#setSize(startingSize + (mouseDown.clientX - event.clientX));
                        }
                        this.#updateTargetProperty();
                    } else if (event.type == "mouseup") {
                        this.classList.remove("dragging");
                        this.#saveSize();
                        break;
                    }
                }
            }
        }
    }
}

customElements.define("rkgk-resize-handle", ResizeHandle);
