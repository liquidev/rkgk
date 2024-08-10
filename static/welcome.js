import { isUserLoggedIn, registerUser } from "./session.js";

export class Welcome extends HTMLElement {
    constructor() {
        super();
    }

    connectedCallback() {
        this.dialog = this.querySelector("dialog[name='welcome-dialog']");
        this.form = this.dialog.querySelector("form");
        this.nicknameField = this.querySelector("input[name='nickname']");
        this.registerButton = this.querySelector("button[name='register']");
        this.registerProgress = this.querySelector("rkgk-throbber[name='register-progress']");

        if (!isUserLoggedIn()) {
            this.dialog.showModal();

            // Require an account to use the website.
            this.dialog.addEventListener("close", (event) => event.preventDefault());

            this.form.addEventListener("submit", async (event) => {
                event.preventDefault();

                this.registerProgress.beginLoading();
                let response = await registerUser(this.nicknameField.value);
                if (response.status != "ok") {
                    this.registerProgress.showError(response.message);
                }

                this.dialog.close();
            });
        }
    }
}

customElements.define("rkgk-welcome", Welcome);
