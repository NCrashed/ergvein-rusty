{ config, lib, ... }:
with lib;  # use the functions from lib, such as mkIf
let
  pkgs = import <nixpkgs> { };
  # the values of the options set for the service by the user of the service
  bitcoin-cfg = config.services.bitcoin;
in {
  ##### interface. here we define the options that users of our service can specify
  options = {
    # the options for our service will be located under services.bitcoin
    services.bitcoin = {
      enable = mkOption {
        type = types.bool;
        default = true;
        description = ''
          Whether to enable bitcoin node by default.
        '';
      };
      package = mkOption {
        type = types.package;
        default = pkgs.bitcoind;
        defaultText = "pkgs.bitcoind";
        description = ''
          Which bitcoin package to use with the service. The bitcoin node have to
          be built with ZeroMQ support.
        '';
      };
      nodeUser = mkOption {
        type = types.str;
        default = "bitcoin";
        description = ''
          Which name of RPC user to use.
        '';
      };
      passwordHMAC = mkOption {
        type = types.uniq (types.strMatching "[0-9a-f]+\\$[0-9a-f]{64}");
        example = "f7efda5c189b999524f151318c0c86$d5b51b3beffbc02b724e5d095828e0bc8b2456e9ac8757ae3211a5d9b16a22ae";
        description = ''
          Password HMAC-SHA-256 for JSON-RPC connections. Must be a string of the
          format &lt;SALT-HEX&gt;$&lt;HMAC-HEX&gt;.
          Tool (Python script) for HMAC generation is available here:
          <link xlink:href="https://github.com/bitcoin/bitcoin/blob/master/share/rpcauth/rpcauth.py"/>
        '';
      };
      nodePort = mkOption {
        type = types.int;
        description = ''
          Which port the cryptonode serves RPC.
        '';
      };
      nodeAddress = mkOption {
      	type = types.str;
        default = "127.0.0.1";
        description = ''
          Which address to listhen for node RPC.
        '';
      };
      nodeZMQPortBlock = mkOption {
        type = types.int;
        default = 29000;
        description = ''
          Which port the cryptonode serves RPC with ZeroMQ protocol. Block API.
        '';
      };
      nodeZMQPortTx = mkOption {
        type = types.int;
        default = 29000;
        description = ''
          Which port the cryptonode serves RPC with ZeroMQ protocol. Transaction API.
        '';
      };
      testnet = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Start in testnet mode. Uses different data dir.
        '';
      };
      reindex = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Start with -reindex flag.
        '';
      };

      datadir = mkOption {
        type = types.str;
        default = if bitcoin-cfg.testnet then "/var/lib/bitcoin-testnet" else "/var/lib/bitcoin";
        description = ''
          Path to blockchain database on filesystem.
        '';
      };

      extraConfig = mkOption {
        type = types.lines;
        default = "";
        example = ''
          par=16
          rpcthreads=16
          logips=1
        '';
        description = "Additional configurations to be appended to <filename>bitcoin.conf</filename>.";
      };

      config = mkOption {
        type = types.str;
        default = ''
          server=1
          rest=1
          txindex=1
          testnet=${if bitcoin-cfg.testnet then "1" else "0"}
          ${if bitcoin-cfg.testnet then "[test]" else ""}
          rpcallowip=${bitcoin-cfg.nodeAddress}
          rpcauth=${bitcoin-cfg.nodeUser}:${bitcoin-cfg.passwordHMAC}
          rpcport=${toString bitcoin-cfg.nodePort}
          zmqpubrawblock=tcp://127.0.0.1:${toString bitcoin-cfg.nodeZMQPortBlock}
          zmqpubrawtx=tcp://127.0.0.1:${toString bitcoin-cfg.nodeZMQPortTx}
          dbcache=2048
          rpcworkqueue=128
          rpcclienttimeout=30

          ${bitcoin-cfg.extraConfig}
        '';

        description = ''
          Configuration file for bitcoin.
        '';
      };
      configPath = mkOption {
        type = types.str;
        default = "/etc/bitcoin.conf";
        description = ''
          Configuration file location for bitcoin.
        '';
      };

    };
  };

  ##### implementation
  config = mkIf bitcoin-cfg.enable { # only apply the following settings if enabled
    # Write configuration file to /etc/bitcoin.conf
    environment.etc."bitcoin.conf" = {
      text = bitcoin-cfg.config; # we can use values of options for this service here
    };
    # User to run the node
    users.users.bitcoin = {
      name = "bitcoin";
      group = "bitcoin";
      description = "bitcoin daemon user";
      home = bitcoin-cfg.datadir;
      isSystemUser = true;
    };
    users.groups.bitcoin = {};
    # Create systemd service
    systemd.services.bitcoin = {
      enable = true;
      description = "Bitcoin node";
      after = ["network.target"];
      wants = ["network.target"];
      script = ''
        chmod 700 ${bitcoin-cfg.datadir}
        ${bitcoin-cfg.package}/bin/bitcoind -datadir=${bitcoin-cfg.datadir} -conf=${bitcoin-cfg.configPath} ${if bitcoin-cfg.reindex then "-reindex" else ""}
      '';
      serviceConfig = {
          Restart = "always";
          RestartSec = 30;
          User = "bitcoin";
        };
      wantedBy = ["multi-user.target"];
    };
    # Init folder for bitcoin data
    system.activationScripts = {
      intbitcoin = {
        text = ''
          if [ ! -d "${bitcoin-cfg.datadir}" ]; then
            mkdir -p ${bitcoin-cfg.datadir}
          fi
        '';
        deps = [];
      };
    };
  };
}
