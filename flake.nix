{
  description = "Adoptium Java";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils }:
    utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        sources = builtins.fromJSON (builtins.readFile ./sources.json);
        buildAdoptLike = with import nixpkgs {system = system;}; name: value:
            let
              cpuName = stdenv.hostPlatform.parsed.cpu.name;
              runtimeDependencies = [
                pkgs.cups
                pkgs.cairo
                pkgs.glib
                pkgs.gtk3
              ];
              runtimeLibraryPath = lib.makeLibraryPath runtimeDependencies;
            in
            stdenv.mkDerivation rec {
              name = "jdk${toString value.major_version}";
              src = builtins.fetchurl {
                url = value.link;
                sha256 = value.sha256;
              };
              version = value.java_version;
              buildInputs = with pkgs; [
                alsa-lib
                fontconfig
                freetype
                stdenv.cc.cc.lib
                xorg.libX11
                xorg.libXext
                xorg.libXi
                xorg.libXrender
                xorg.libXtst
                zlib
              ];
              nativeBuildInputs = with pkgs; [
                autoPatchelfHook
                makeWrapper
              ];
              dontStrip = 1;
              installPhase = ''
                cd ..
                mv $sourceRoot $out
                # jni.h expects jni_md.h to be in the header search path.
                ln -s $out/include/linux/*_md.h $out/include/
                rm -rf $out/demo
                # Remove some broken manpages.
                rm -rf $out/man/ja*
                # Remove embedded freetype to avoid problems like
                # https://github.com/NixOS/nixpkgs/issues/57733
                find "$out" -name 'libfreetype.so*' -delete
                # Propagate the setJavaClassPath setup hook from the JDK so that
                # any package that depends on the JDK has $CLASSPATH set up
                # properly.
                mkdir -p $out/nix-support
                printWords ${setJavaClassPath} > $out/nix-support/propagated-build-inputs
                # Set JAVA_HOME automatically.
                cat <<EOF >> "$out/nix-support/setup-hook"
                if [ -z "\''${JAVA_HOME-}" ]; then export JAVA_HOME=$out; fi
                EOF
                # We cannot use -exec since wrapProgram is a function but not a command.
                #
                # jspawnhelper is executed from JVM, so it doesn't need to wrap it, and it
                # breaks building OpenJDK (#114495).
                for bin in $( find "$out" -executable -type f -not -name jspawnhelper ); do
                  if patchelf --print-interpreter "$bin" &> /dev/null; then
                    wrapProgram "$bin" --prefix LD_LIBRARY_PATH : "${runtimeLibraryPath}"
                  fi
                done
              '';
              preFixup = ''
                find "$out" -name libfontmanager.so -exec \
                  patchelf --add-needed libfontconfig.so {} \;
              '';
            };
      in
      with import nixpkgs { system = system; };
      {
        packages.adoptium = (builtins.mapAttrs
          (name: value:
            buildAdoptLike name value
          )
          sources.${system}.adoptium.versions) // {
            latest = buildAdoptLike "latest" sources.${system}.adoptium.latest;
            stable = buildAdoptLike "stable" sources.${system}.adoptium.stable;
            lts = buildAdoptLike "lts" sources.${system}.adoptium.lts;
          };
        
        packages.adoptium-latest = self.packages.${system}.adoptium.latest;
        packages.adoptium-stable = self.packages.${system}.adoptium.stable;
        packages.adoptium-lts = self.packages.${system}.adoptium.lts;
        
        defaultPackage = self.packages.${system}.stable;
      });
}
