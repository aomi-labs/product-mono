// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

struct PackedUserOperation {
    address sender;
    uint256 nonce;
    bytes initCode;
    bytes callData;
    bytes32 accountGasLimits;
    uint256 preVerificationGas;
    bytes32 gasFees;
    bytes paymasterAndData;
    bytes signature;
}

contract DebugHash {
    event PackedEncoded(bytes data);
    event PackedHash(bytes32 hash);

    function debugGetUserOpHash(PackedUserOperation calldata userOp) external returns (bytes32) {
        bytes memory encoded = abi.encode(userOp);
        emit PackedEncoded(encoded);

        bytes32 hash = keccak256(encoded);
        emit PackedHash(hash);

        return hash;
    }
}
