import { Result } from './result.type'

type VerifyUserDto = {
    username?: string
    email?: string
    password: string
}

type AuthPayload = {
    userId: string
}

type VerifyUserResult = Result<AuthPayload>


export { AuthPayload, VerifyUserDto, VerifyUserResult }

