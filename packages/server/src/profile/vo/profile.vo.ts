import { UserVo } from 'src/user/vo/user.vo'

export class ProfileVo {
    name?: string
    avatar?: string
    description?: string
    user: UserVo
}
