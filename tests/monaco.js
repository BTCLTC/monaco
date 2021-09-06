const anchor = require("@project-serum/anchor");

describe("monaco", () => {
  // IDL
  const idl = JSON.parse(require("fs").readFileSync("../target/idl/monaco.json", "utf8"));

  // DEVNET Address
  const programId = new anchor.web3.PublicKey("DXxoamtnFQ2Qg8WeSoF5ezgEZ6fST9iQTbpxkuFAr7Ld");
  anchor.setProvider(anchor.Provider.env());
  const program = new anchor.Program(idl, programId);

  it("Is initialized!", async () => {
    // console.log(program.);
    await program.rpc.initialize();
  });
});
