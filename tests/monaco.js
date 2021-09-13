const anchor = require("@project-serum/anchor");

describe("monaco", () => {
  // Program idl
  const idl = JSON.parse(require("fs").readFileSync("../target/idl/monaco.json", "utf8"));

  // Devnet program initialisation
  const programId = new anchor.web3.PublicKey("394ZuAHtmQhHot1NzC4f5Q1UD2wWPvjz2N5RtWJr5Yo3");
  anchor.setProvider(anchor.Provider.env());
  const program = new anchor.Program(idl, programId);

  const main_wallet_priv = [
    150, 62, 212, 97, 151, 171, 147, 120, 245, 181, 97, 145, 242, 7, 197, 212, 100, 34, 39, 168, 159, 67, 26, 172, 248,
    105, 62, 152, 179, 106, 232, 0, 56, 216, 207, 211, 124, 95, 156, 132, 103, 126, 100, 122, 111, 154, 158, 8, 180,
    233, 126, 41, 252, 211, 164, 230, 163, 33, 5, 154, 138, 158, 14, 186,
  ];

  const main_wallet = anchor.web3.Keypair.fromSecretKey(Uint8Array.from(main_wallet_priv));

  it("Test deposit", async () => {
    await program.rpc.deposit();
  });
});
