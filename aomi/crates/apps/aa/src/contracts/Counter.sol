// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

/**
 * @title Counter
 * @notice Simple test contract for verifying smart account execution
 */
contract Counter {
    uint256 public value;

    event Incremented(uint256 newValue);
    event Decremented(uint256 newValue);

    function increment() external {
        value++;
        emit Incremented(value);
    }

    function decrement() external {
        require(value > 0, "Counter: cannot decrement below zero");
        value--;
        emit Decremented(value);
    }

    function getValue() external view returns (uint256) {
        return value;
    }

    function setValue(uint256 newValue) external {
        value = newValue;
    }
}
