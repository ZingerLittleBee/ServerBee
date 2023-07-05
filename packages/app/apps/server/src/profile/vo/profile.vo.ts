import { UserVo } from '@app/server/user/vo/user.vo'

export class ProfileVo {
    name?: string
    avatar?: string
    description?: string
    user: UserVo
}
