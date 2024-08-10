export class OnlineUsers extends EventTarget {
    #users = new Map();

    constructor() {
        super();
    }

    addUser(sessionId, userInfo) {
        this.#users.set(sessionId, userInfo);
    }

    getUser(sessionId) {
        return this.#users.get(sessionId);
    }

    removeUser(sessionId) {
        this.#users.delete(sessionId);
    }
}
