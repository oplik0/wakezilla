{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.wakezilla;
  wakezilla-pkg = pkgs.wakezilla;
in
{
  options.services.wakezilla = {
    enable = mkEnableOption "wakezilla service";

    proxy = {
      enable = mkEnableOption "wakezilla proxy server";
      port = mkOption {
        type = types.port;
        default = 3000;
        description = "Port to listen on for the proxy server.";
      };
    };

    client = {
      enable = mkEnableOption "wakezilla client server";
      port = mkOption {
        type = types.port;
        default = 3001;
        description = "Port to listen on for the client server.";
      };
    };

    package = mkOption {
      type = types.package;
      default = wakezilla-pkg;
      description = "The wakezilla package to use.";
    };
  };

  config = mkIf cfg.enable {
    systemd.services.wakezilla-proxy = mkIf cfg.proxy.enable {
      description = "Wakezilla Proxy Server";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/wakezilla proxy-server --port ${toString cfg.proxy.port}";
        Restart = "on-failure";
        User = "wakezilla";
        Group = "wakezilla";
      };
    };

    systemd.services.wakezilla-client = mkIf cfg.client.enable {
      description = "Wakezilla Client Server";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/wakezilla client-server --port ${toString cfg.client.port}";
        Restart = "on-failure";
        User = "wakezilla";
        Group = "wakezilla";
      };
    };

    users.users.wakezilla = {
      isSystemUser = true;
      group = "wakezilla";
    };

    users.groups.wakezilla = {};
  };
}
