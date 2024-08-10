const defaultBrush = `
; This is your brush.
; Feel free to edit it to your liking!
(stroke
    8                       ; thickness
    (rgba 0.0 0.0 0.0 1.0)  ; color
    (vec))                  ; position
`.trim();

export class BrushEditor extends HTMLElement {
    constructor() {
        super();
    }

    connectedCallback() {
        this.classList.add("rkgk-panel");

        this.textArea = this.appendChild(document.createElement("pre"));
        this.textArea.classList.add("text-area");
        this.textArea.textContent = defaultBrush;
        this.textArea.contentEditable = true;
        this.textArea.spellcheck = false;
        this.textArea.addEventListener("input", () => {
            this.dispatchEvent(
                Object.assign(new Event(".codeChanged"), {
                    newCode: this.textArea.value,
                }),
            );
        });
    }

    get code() {
        return this.textArea.textContent;
    }
}

customElements.define("rkgk-brush-editor", BrushEditor);
