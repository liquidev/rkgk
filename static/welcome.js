export class Welcome extends HTMLElement {
    constructor() {
        super();
    }

    connectedCallback() {
        this.welcomeDialog = this.querySelector("dialog[name='welcome-dialog']");
        this.welcomeForm = this.welcomeDialog.querySelector("form");
        this.nicknameField = this.querySelector("input[name='nickname']");
        this.registerButton = this.querySelector("button[name='register']");
        this.registerProgress = this.querySelector("rkgk-throbber[name='register-progress']");

        // Once the dialog is open, you need an account to use the website.
        this.welcomeDialog.addEventListener("cancel", (event) => event.preventDefault());
    }

    show({ onRegister }) {
        let resolvePromise;
        let promise = new Promise((resolve) => (resolvePromise = resolve));

        this.welcomeDialog.showModal();

        let submitListener = async (event) => {
            event.preventDefault();

            this.registerProgress.beginLoading();
            let response = await onRegister(this.nicknameField.value);
            if (response.status == "ok") {
                this.welcomeDialog.close();
                resolvePromise();
            } else {
                this.registerProgress.showError(response.message);
            }
            this.welcomeForm.removeEventListener("submit", submitListener);
        };
        this.welcomeForm.addEventListener("submit", submitListener);

        return promise;
    }
}

customElements.define("rkgk-welcome", Welcome);
