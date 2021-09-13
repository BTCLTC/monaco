# Monaco

Monaco is a DCA protocol for solana built on top of Serum and compatible with any program that implements or extends the instruction interface from the standard spl token-lending program.

## What is it comprised of?

The protocol itself is made up of 4 parts:

1. The smart contract (or program, whatever you like to call it)
2. An offchain scheduling server that performs the DCA purchases at the specified intervals
3. A CPI library for anchor: `anchor-lending`. You can find this on my Github profile with a simple search
4. A frontend website to interact with the protocol

## How does it work?

Monaco is simple. It takes in deposits and uses them to provide liquidity on whichever lending protocol is chosen. On the specified intervals, it will extract _ONLY_ the profits made on the liquidity deposit and will use it to fund a purchase of a specified token on Serum.

The idea here is for there to be a seamless way to stack up on the tokens you want to build up a position on over time but without having to supply more money again and again.

With the Monaco way, the base capital is left untouched while the interest earned over time is used to size up your positions.

## Is it done yet?

No. As of the time I'm writing this, I have only finished the initial smart contract, the `anchor-lending` CPI library, and have begun preliminary work on the offchain scheduling server. After completing that, I will also need to build out the frontend.

## Will this be supported after the hackathon is over?

Yes, v1 will simply be DCA via lending protocols and anchor. v2 will be off-the-walls crazy.

## Will there be a token?

Honestly idk. I don't want to shove in a token unless there is a really good reason to. Right now, token incentives seem somewhat parasitic to me (just an opinion) but maybe I'll change my mind on this later, I really have no idea. Please DM me if you have thoughts on this and wanna talk about it

## Is there a sneak preview of the UI?

Yes, here it is. Try and take it with a grain of salt though. This is very early stages and things might change :)

![ui-sample](https://i.imgur.com/bN6r9js.png)

---

Feel free to dm me on twitter if you have any questions about what I'm doing or if you want to help out somehow :)
