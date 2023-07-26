const serverBasePath = "/s"
const clientBasePath = "/c"
const loginUrl = `${serverBasePath}/auth/login`
const registerUrl = `${serverBasePath}/auth/register`
const logoutUrl = `${serverBasePath}/auth/logout`

const profileUrl = `${serverBasePath}/profile`

const addClientUrl = `${clientBasePath}/client`

export {
    loginUrl,
    registerUrl,
    logoutUrl,
    profileUrl
}
