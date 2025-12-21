{
  description = "NixOS WoL Raspberry Pi Configuration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    nixos-raspberrypi.url = "github:nvmd/nixos-raspberrypi/main";
    sops-nix.url = "github:Mic92/sops-nix";
    # Use the local parent directory for wakezilla during development
    wakezilla.url = "path:..";
    
    # SSH host keys directory - override this input to point to your keys
    # Default expects keys in ./keys/ directory
    # Build with: nix build --impure .#nixosConfigurations.rpi.config.system.build.images.sd-card
    host-keys = {
      url = "path:./keys";
      flake = false;
    };
  };

  nixConfig = {
    extra-substituters = [
      "https://nixos-raspberrypi.cachix.org"
    ];
    extra-trusted-public-keys = [
      "nixos-raspberrypi.cachix.org-1:4iMO9LXa8BqhU+Rpg6LQKiGa2lsNh/j2oiYLNOQ5sPI="
    ];
  };

  outputs = { self, nixpkgs, nixos-raspberrypi, sops-nix, wakezilla, host-keys }:
  {
    nixosConfigurations.rpi = nixos-raspberrypi.lib.nixosSystem {
      specialArgs = { inherit self nixpkgs nixos-raspberrypi wakezilla host-keys; };
      system = "aarch64-linux";
      modules = [
        sops-nix.nixosModules.sops
        wakezilla.nixosModules.wakezilla
        {
          # Hardware specific configuration
          imports = with nixos-raspberrypi.nixosModules; [
            raspberry-pi-3.base
          ];
        }
        ({ config, pkgs, lib, wakezilla, host-keys, ... }: {
          # Basic system configuration
          system.stateVersion = "25.11";

          system.nixos.tags = let
            cfg = config.boot.loader.raspberryPi;
          in [
            "raspberry-pi-${cfg.variant}"
            cfg.bootloader
            config.boot.kernelPackages.kernel.version
          ];

          # ============================================================
          # SOPS-NIX SECRETS CONFIGURATION
          # ============================================================
          # Secrets are encrypted with age using the SSH host key.
          # The host key is pre-generated and baked into the image so
          # secrets can be decrypted immediately on first boot.
          #
          # Setup (one-time):
          # 1. Generate host keys: ./scripts/generate-host-keys.sh
          # 2. Get the age public key: cat keys/ssh_host_ed25519_key.pub | nix run nixpkgs#ssh-to-age
          # 3. Add the age key to .sops.yaml
          # 4. Create/edit secrets: nix run nixpkgs#sops secrets/secrets.yaml
          # ============================================================
          
          sops = {
            defaultSopsFile = ./secrets/secrets.yaml;
            
            # Use the pre-generated SSH host key for decryption
            age.sshKeyPaths = [ "/etc/ssh/ssh_host_ed25519_key" ];
            
            # Define the secrets we need
            secrets = {
              # WiFi environment file (contains WIFI_TEST_PSK and WIFI_TRISTATE_PSK)
              "wifi/env" = {};
              
              "cloudflare/tunnelId" = {};
              "cloudflare/tunnelCredentials" = {
                # Write credentials to the cloudflared config location
                path = "/etc/cloudflared/credentials.json";
                owner = "cloudflared";
                group = "cloudflared";
                mode = "0600";
              };
              "cloudflare/domain" = {};
            };
          };

          # Use pre-generated SSH host keys (required for sops-nix to work on first boot)
          services.openssh.hostKeys = [
            {
              path = "/etc/ssh/ssh_host_ed25519_key";
              type = "ed25519";
            }
          ];

          # Copy pre-generated host keys into the image
          # The private key file must exist at ./keys/ssh_host_ed25519_key
          # Generate with: ../scripts/generate-host-keys.sh
          # 
          # NOTE: Build with --impure flag to access local key files:
          #   nix build .#nixosConfigurations.rpi.config.system.build.images.sd-card --impure
          environment.etc."ssh/ssh_host_ed25519_key" = {
            source = "${host-keys}/ssh_host_ed25519_key";
            mode = "0600";
          };
          environment.etc."ssh/ssh_host_ed25519_key.pub" = {
            source = "${host-keys}/ssh_host_ed25519_key.pub";
            mode = "0644";
          };

          # Enable redistributable firmware (required for WiFi)
          hardware.enableRedistributableFirmware = true;

          # WiFi setup using secrets from sops
          # The secretsFile contains environment variables referenced by pskRaw
          networking = {
            hostName = "wol-rpi";
            wireless = {
              enable = true;
              # secretsFile is read at runtime and provides PSK values
              secretsFile = config.sops.secrets."wifi/env".path;
              networks = {
                "test" = {
                  pskRaw = "ext:WIFI_TEST_PSK";
                };
                "TriState" = {
                  pskRaw = "ext:WIFI_TRISTATE_PSK";
                };
              };
            };
          };

          # Enable SSH
          services.openssh = {
            enable = true;
            settings = {
              PermitRootLogin = "yes";
              # Required for cloudflared browser rendering support
              Macs = [
                "hmac-sha2-512-etm@openssh.com"
                "hmac-sha2-256-etm@openssh.com"
                "umac-128-etm@openssh.com"
                "hmac-sha2-256"
              ];
            };
          };

          # ============================================================
          # Cloudflare Tunnel Configuration
          # ============================================================
          # Before building the image:
          # 1. Install cloudflared: nix-shell -p cloudflared
          # 2. Login: cloudflared tunnel login
          # 3. Create tunnel: cloudflared tunnel create wol-rpi
          # 4. Note the tunnel ID and copy the credentials JSON content
          # 5. Add CNAME record in Cloudflare DNS:
          #    <your-subdomain> -> <tunnel-id>.cfargotunnel.com
          # 6. Add secrets to secrets/secrets.yaml using sops
          # ============================================================

          # Cloudflare tunnel - configured via a systemd service that reads secrets
          # We use a wrapper service since cloudflared config needs the tunnel ID
          systemd.services.cloudflared-tunnel = {
            description = "Cloudflare Tunnel";
            after = [ "network-online.target" "sops-nix.service" ];
            wants = [ "network-online.target" ];
            wantedBy = [ "multi-user.target" ];
            
            serviceConfig = {
              User = "cloudflared";
              Group = "cloudflared";
              ExecStart = pkgs.writeShellScript "cloudflared-start" ''
                TUNNEL_ID=$(cat ${config.sops.secrets."cloudflare/tunnelId".path})
                DOMAIN=$(cat ${config.sops.secrets."cloudflare/domain".path})
                
                # Create tunnel config
                cat > /run/cloudflared/config.yaml <<EOF
                tunnel: $TUNNEL_ID
                credentials-file: ${config.sops.secrets."cloudflare/tunnelCredentials".path}
                ingress:
                  - hostname: $DOMAIN
                    service: http://localhost:${toString config.services.wakezilla.proxy.port}
                  - service: http_status:404
                EOF
                
                exec ${pkgs.cloudflared}/bin/cloudflared tunnel --config /run/cloudflared/config.yaml run
              '';
              RuntimeDirectory = "cloudflared";
              Restart = "on-failure";
              RestartSec = "5s";
            };
          };

          # Ensure cloudflared user/group exist
          users.users.cloudflared = {
            isSystemUser = true;
            group = "cloudflared";
          };
          users.groups.cloudflared = {};

          # Development and administration tools
          environment.systemPackages = with pkgs; [
            # Basic tools
            vim
            nano
            git
            htop
            tmux
            wget
            curl
            
            # Network tools
            nmap
            tcpdump
            iperf
            dig
            
            # System tools
            lsof
            usbutils
            pciutils
            
            # Development tools
            gcc
            gnumake
            python3

            # Wake-on-LAN tool
            wol
            
            # Secrets management
            sops
            age
          ];

          # User configuration
          users.users.tristate = {
            isNormalUser = true;
            extraGroups = [ "wheel" "networkmanager" ];
            initialPassword = "zaq1@WSX"; # Change this password after first login!
          };

          # Allow sudo without password for wheel group
          security.sudo.wheelNeedsPassword = false;

          # Bootloader configuration is handled by raspberry-pi-3.base module
          # No need to manually configure boot.loader.raspberryPi

          # Enable serial console (useful for debugging)
          boot.kernelParams = [ "console=ttyS1,115200n8" ];

          # ============================================================
          # Wakezilla Service Configuration
          # ============================================================
          services.wakezilla = {
            enable = true;
            
            # Use the wakezilla package from the flake input
            package = wakezilla.packages.aarch64-linux.wakezilla;
            
            proxy = {
              enable = true;
              port = 3000;
            };
            
            # Enable client server if you need local WoL capabilities
            # client = {
            #   enable = true;
            #   port = 3001;
            # };
          };

          # Firewall configuration
          networking.firewall = {
            enable = true;
            allowedTCPPorts = [
              22  # SSH
              config.services.wakezilla.proxy.port
            ];
          };
        })
      ];
    };
  };
}
