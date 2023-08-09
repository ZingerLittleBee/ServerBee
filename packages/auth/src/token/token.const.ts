enum SignType {
    client,
    user
}

enum TokenType {
    userAccess,
    userRefresh,
    clientAccess,
    clientRefresh
}

const kUserAccessSecret = 'JWT_USER_ACCESS_SECRET'

const kUserAccessExpiration = 'JWT_USER_ACCESS_EXPIRATION'

const kUserRefreshSecret = 'JWT_USER_REFRESH_SECRET'

const kUserRefreshExpiration = 'JWT_USER_REFRESH_'

const kClientAccessSecret = 'JWT_CLIENT_ACCESS_SECRET'

const kClientAccessExpiration = 'JWT_CLIENT_ACCESS_EXPIRATION'

const kClientRefreshSecret = 'JWT_CLIENT_REFRESH_SECRET'

const kClientRefreshExpiration = 'JWT_CLIENT_REFRESH_EXPIRATION'

const kSaltRounds = 'SALT_ROUNDS'

const defaultClientAccessExpiration = 3600 * 24 * 30
const defaultClientRefreshExpiration = 3600 * 24 * 30 * 6
const defaultUserAccessExpiration = 3600 * 24 * 30
const defaultUserRefreshExpiration = 3600 * 24 * 30 * 6
const defaultSaltRounds = 10

export {
    SignType,
    TokenType,
    kClientAccessSecret,
    kClientAccessExpiration,
    kClientRefreshSecret,
    kClientRefreshExpiration,
    kUserAccessSecret,
    kUserAccessExpiration,
    kUserRefreshSecret,
    kUserRefreshExpiration,
    kSaltRounds,
    defaultClientAccessExpiration,
    defaultClientRefreshExpiration,
    defaultUserRefreshExpiration,
    defaultSaltRounds,
    defaultUserAccessExpiration
}
