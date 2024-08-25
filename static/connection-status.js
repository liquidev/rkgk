export class ConnectionStatus extends HTMLElement {
    connectedCallback() {
        this.loggingInDialog = this.querySelector("dialog[name='logging-in-dialog']");
        this.loggingInThrobber = this.loggingInDialog.querySelector("rkgk-throbber");

        // This is a progress dialog and shouldn't be closed.
        this.loggingInDialog.addEventListener("cancel", (event) => event.preventDefault());

        this.errorDialog = this.querySelector("dialog[name='error-dialog']");
        this.errorText = this.errorDialog.querySelector("[name='error-text']");
        this.errorRefresh = this.errorDialog.querySelector("button[name='refresh']");

        // If this appears then something broke, and therefore the app can't continue normally.
        this.errorDialog.addEventListener("cancel", (event) => event.preventDefault());

        this.errorRefresh.addEventListener("click", () => {
            window.location.reload();
        });

        this.disconnectedDialog = this.querySelector("dialog[name='disconnected-dialog']");
        this.reconnectDuration = this.disconnectedDialog.querySelector(
            "[name='reconnect-duration']",
        );

        // If this appears then we can't let the user use the app, because we're disconnected.
        this.disconnectedDialog.addEventListener("cancel", (event) => event.preventDefault());
    }

    showLoggingIn() {
        this.loggingInDialog.showModal();
    }

    hideLoggingIn() {
        this.loggingInDialog.close();
    }

    showError(error) {
        this.errorDialog.showModal();
        if (error instanceof Error) {
            if (error.stack != null && error.stack != "") {
                this.errorText.textContent = `${error.toString()}\n\n${error.stack}`;
            } else {
                this.errorText.textContent = error.toString();
            }
        }
    }

    async showDisconnected(duration) {
        this.disconnectedDialog.showModal();

        let updateDuration = (remaining) => {
            let seconds = Math.floor(remaining / 1000);
            this.reconnectDuration.textContent = `${seconds} ${seconds == 1 ? "second" : "seconds"}`;
        };

        let remaining = duration;
        updateDuration(remaining);
        while (remaining > 0) {
            let delay = Math.min(1000, remaining);
            remaining -= delay;
            updateDuration(remaining);
            await new Promise((resolve) => setTimeout(resolve, delay));
        }
    }
}

customElements.define("rkgk-connection-status", ConnectionStatus);
