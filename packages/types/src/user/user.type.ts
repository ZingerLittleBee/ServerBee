type CreateUser = {
    username: string
    password: string
    email: string
}

type UserVo = {
    userId: string
    username: string
    email: string
}

type FindUserDto = {
    username?: string
    email?: string
    userId?: string
}

type UserPayload = {
    userId: string
}


export { CreateUser, FindUserDto, UserVo, UserPayload }

