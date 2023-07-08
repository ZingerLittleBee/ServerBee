import { Result } from "./result.type"

type VertifyUserDto = {
    username?: string
    email?: string
    password: string
}

type AuthPayload = {
    userId: string
}

type VertifyUserResult = Result<AuthPayload>


export { AuthPayload, VertifyUserDto, VertifyUserResult }

