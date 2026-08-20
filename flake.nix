{
  description = "WalPad 8G recovery + server toolchain";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = nixpkgs.legacyPackages.${system};
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            python3
            python3.pkgs.pyusb
            python3.pkgs.pyserial
            python3.pkgs.capstone
            python3.pkgs.pillow
            git
            android-tools      # adb + fastboot
            libusb1
            udev
          ];
          shellHook = ''
            echo "WalPad 8G toolchain. Install mtkclient: make setup"
          '';
        };
      });
}