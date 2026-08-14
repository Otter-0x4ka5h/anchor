import { spawnSync } from "child_process";

describe("remaining-accounts-uaf", () => {
  it("rejects storing local account references into remaining account RefCells", () => {
    const result = spawnSync("anchor", [
      "build",
      "--no-idl",
      "--ignore-keys",
      "-p",
      "remaining-accounts-uaf",
    ]);
    if (result.status === 0) {
      throw new Error("Lifetime escape build unexpectedly succeeded");
    }

    const output = result.output.toString();
    if (
      !output.includes("lifetime may not live long enough") &&
      !output.includes("does not live long enough")
    ) {
      throw new Error(
        `Lifetime escape build did not return the expected error: "${output}"`
      );
    }
  });
});
