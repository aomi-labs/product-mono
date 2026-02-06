// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

import "@openzeppelin/contracts/utils/Create2.sol";
import "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import "./SimpleAccount.sol";

/**
 * @title SimpleAccountFactory
 * @notice Factory for deploying SimpleAccount instances using CREATE2
 * @dev Uses ERC1967Proxy for upgradeable accounts
 */
contract SimpleAccountFactory {
    SimpleAccount public immutable accountImplementation;

    event AccountCreated(address indexed account, address indexed owner, uint256 salt);

    constructor(IEntryPoint _entryPoint) {
        accountImplementation = new SimpleAccount(_entryPoint);
    }

    /**
     * @notice Create an account and return its address
     * @dev This method returns an existing account address if it has already been deployed
     * @param owner The owner address
     * @param salt Unique salt for CREATE2
     * @return account The deployed or existing account address
     */
    function createAccount(address owner, uint256 salt) public returns (SimpleAccount account) {
        address addr = getAddress(owner, salt);
        uint256 codeSize = addr.code.length;

        if (codeSize > 0) {
            return SimpleAccount(payable(addr));
        }

        // Deploy proxy with initialization data
        bytes memory initData = abi.encodeCall(SimpleAccount.initialize, (owner));
        account = SimpleAccount(
            payable(
                new ERC1967Proxy{salt: bytes32(salt)}(
                    address(accountImplementation),
                    initData
                )
            )
        );

        emit AccountCreated(address(account), owner, salt);
    }

    /**
     * @notice Calculate the counterfactual address of an account
     * @param owner The owner address
     * @param salt Unique salt for CREATE2
     * @return The counterfactual address
     */
    function getAddress(address owner, uint256 salt) public view returns (address) {
        bytes memory initData = abi.encodeCall(SimpleAccount.initialize, (owner));

        return Create2.computeAddress(
            bytes32(salt),
            keccak256(
                abi.encodePacked(
                    type(ERC1967Proxy).creationCode,
                    abi.encode(address(accountImplementation), initData)
                )
            )
        );
    }
}
