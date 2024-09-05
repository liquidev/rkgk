import { CodeEditor } from "rkgk/code-editor.js";

const defaultBrush = `
-- This is your brush.
-- Try playing around with the numbers,
-- and see what happens!

stroke 8 #000 (vec 0 0)
`.trim();

export class BrushEditor extends HTMLElement {
    constructor() {
        super();
    }

    connectedCallback() {
        this.classList.add("rkgk-panel");

        this.codeEditor = this.appendChild(new CodeEditor());
        this.codeEditor.setCode(localStorage.getItem("rkgk.brushEditor.code") ?? defaultBrush);
        this.codeEditor.addEventListener(".codeChanged", (event) => {
            this.dispatchEvent(
                Object.assign(new Event(".codeChanged"), {
                    newCode: event.newCode,
                }),
            );
        });

        this.errorHeader = this.appendChild(document.createElement("h1"));
        this.errorHeader.classList.add("error-header");

        this.errorArea = this.appendChild(document.createElement("pre"));
        this.errorArea.classList.add("errors");
    }

    get code() {
        return this.codeEditor.code;
    }

    resetErrors() {
        this.errorHeader.textContent = "";
        this.errorArea.textContent = "";
    }

    renderHakuResult(phase, result) {
        this.resetErrors();

        if (result.status != "error") return;

        this.errorHeader.textContent = `${phase} failed`;

        if (result.errorKind == "diagnostics") {
            // This is kind of wooden; I'd prefer if the error spans were rendered inline in text,
            // but I haven't integrated anything for syntax highlighting yet that would let me
            // do that.
            this.errorArea.textContent = result.diagnostics
                .map(
                    (diagnostic) => `${diagnostic.start}..${diagnostic.end}: ${diagnostic.message}`,
                )
                .join("\n");
        } else if (result.errorKind == "plain") {
            this.errorHeader.textContent = result.message;
        } else if (result.errorKind == "exception") {
            // TODO: This should show a stack trace.
            this.errorArea.textContent = `an exception occurred: ${result.message}`;
        } else {
            console.warn(`unknown error kind: ${result.errorKind}`);
            this.errorHeader.textContent = "(unknown error kind)";
        }
    }
}

customElements.define("rkgk-brush-editor", BrushEditor);
