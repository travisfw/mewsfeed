{
  description = "Flake for Holochain app development";

  inputs = {
    holonix.url = "github:holochain/holonix?ref=main-0.6";

    nixpkgs.follows = "holonix/nixpkgs";
    flake-parts.follows = "holonix/flake-parts";
  };

  outputs = inputs@{ flake-parts, ... }: flake-parts.lib.mkFlake { inherit inputs; } {
    systems = builtins.attrNames inputs.holonix.devShells;
    perSystem = { inputs', pkgs, ... }: {
      formatter = pkgs.nixpkgs-fmt;

      devShells.default = pkgs.mkShell {
        inputsFrom = [ inputs'.holonix.devShells ];

        packages = (with inputs'.holonix.packages; [
          holochain
          bootstrap-srv
          lair-keystore
          hc
          hc-scaffold
          hn-introspect
          rust # For Rust development, with the WASM target included for zome builds
        ]) ++ (with pkgs; [
          nodejs_22 # For UI development
          binaryen # For WASM optimisation
          cmake # For holochain sweettest integration tests
          llvmPackages.libclang # For bindgen (sweettest dependencies)
          zlib #For datachannel-sys (sweettest dependency)
          zlib.dev # CMake needs the dev package too
          xz # For liblzma (sweettest runtime dependency)
          # Add any other packages you need here
        ]);

        shellHook = ''
          export PS1='\[\033[1;34m\][holonix:\w]\$\[\033[0m\] '
          export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
          export LD_LIBRARY_PATH="${pkgs.xz.out}/lib:$LD_LIBRARY_PATH"
        '';
      };
    };
  };
}
