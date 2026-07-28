const NodeEnv = require('jest-environment-node').TestEnvironment;

module.exports = class ReactNativeEnv extends NodeEnv {
  customExportConditions = ['require', 'react-native'];
};
