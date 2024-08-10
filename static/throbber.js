export class Throbber extends HTMLElement {
    constructor() {
        super();
    }

    connectedCallback() {}

    beginLoading() {
        this.className = "loading";
    }

    showError(message) {
        this.className = "error";
        this.textContent = message;
    }
}

customElements.define("rkgk-throbber", Throbber);
