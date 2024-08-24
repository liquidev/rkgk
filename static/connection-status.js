export class ConnectionStatus extends HTMLElement {
    connectedCallback() {
        this.loggingInDialog = this.querySelector("dialog[name='logging-in-dialog']");
        this.loggingInThrobber = this.loggingInDialog.querySelector("rkgk-throbber");

        // This is a progress dialog and shouldn't be closed.
        this.loggingInDialog.addEventListener("cancel", (event) => event.preventDefault());
    }

    showLoggingIn() {
        this.loggingInDialog.showModal();
    }

    hideLoggingIn() {
        this.loggingInDialog.close();
    }
}

customElements.define("rkgk-connection-status", ConnectionStatus);
