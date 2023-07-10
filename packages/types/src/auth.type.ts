import { Result } from './result.type'

type VerifyUserDto = {
    username?: string
    email?: string
    password: string
}

type AuthPayload = {
    userId: string
}

type VertifyUserResult = Result<AuthPayload>


export { AuthPayload, VerifyUserDto, VertifyUserResult }

