{{ #include ../../links.md }}

# Network

> This is implemented in [`net`][kernel_net]

The network and its components.

Here, we have the structs and resources that can be used to communicate over the network:

- [`EthernetSocket`][kernel_net_eth_socket] - A socket that can send and receive packets over the network.
- [`NetworkPacket`][kernel_net_packet] - Stack based packet structure that holds several [`NetworkHeader`][kernel_net_header]s along with payload. The socket can read/write to this struct efficiently, and due to its implementation, it can be used multiple times to send and receive packets.
