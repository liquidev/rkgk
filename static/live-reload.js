// NOTE: The server never fulfills this request, it stalls forever.
// Once the connection is closed, we try to connect with the server until we establish a successful
// connection. Then we reload the page.
await fetch("/auto-reload/stall").catch(async () => {
    while (true) {
        try {
            let response = await fetch("/auto-reload/back-up");
            if (response.status == 200) {
                window.location.reload();
                break;
            }
        } catch (e) {
            await new Promise((resolve) => setTimeout(resolve, 100));
        }
    }
});
