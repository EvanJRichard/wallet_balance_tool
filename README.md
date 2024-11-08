## Summary of use
`cargo run`

This will prompt you for the vpub. Click `Check Balances` and the app will check 10 addresses + 1 change address (10 `m/0/0` + 1 `m/1/0`). Click `Load More Balances` to load another 10+1. The cumulative balance updates each time.

### Commentary
This was really fun. I haven't used tokio and iced, so I tried to incorporate them, it was a cool learning experience. There's loads I'd love to fix (there's a bug where the app would crash on resize, so I disabled resizing; the app could probably automatically figure out how many addresses you use like how Electrum does, rather than loading 11 at a time manually; it would be nice to be able to configure what data providers are used, and with what rate limits; I wish you could copy and paste the output data easier; and a lot more) but it's been a week so I'll send in what I have.

### Thank you!