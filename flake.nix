{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      muslPkgs = pkgs.pkgsStatic;
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          rustup
          musl
        ];

        shellHook = ''
          rustup target add x86_64-unknown-linux-musl 2>/dev/null
          export CC_x86_64_unknown_linux_musl="musl-gcc"
          export TARGET_CC="musl-gcc"
        '';
      };

      packages.${system}.default = muslPkgs.rustPlatform.buildRustPackage {
        pname = cargoToml.package.name;
        version = cargoToml.package.version;
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;

        # Integration tests shell out to git
        nativeCheckInputs = [ pkgs.git ];

        # Push tests need a git identity for commits
        preCheck = ''
          export HOME="$TMPDIR"
          git config --global user.email "nix-build@localhost"
          git config --global user.name "nix-build"
        '';
      };
    };
}
