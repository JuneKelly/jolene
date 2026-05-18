{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      muslPkgs = pkgs.pkgsStatic;
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
        pname = "jolene";
        version = "0.1.7";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;
      };
    };
}
